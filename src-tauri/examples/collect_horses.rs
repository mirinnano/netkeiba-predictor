//! データセットに出てくる馬の競走成績（db.netkeiba.com）を集めて JSON Lines に保存する。
//!
//!   cargo run --example collect_horses -- <出力先.jsonl> <races.jsonl>...
//!
//! 並列数は環境変数 WORKERS（既定 1, 最大 5）。各ワーカー2秒間隔。失敗が5回続いたら止まる。途中で止めても保存済みの馬は次回飛ばす。
//! ページには予測対象のレースより後の走も載るので、使う側で日付を必ず絞ること。
use netkeiba_predictor_lib::scraper;
use std::collections::HashSet;
use std::io::{BufRead, Write};

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(run());
}

async fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().expect("出力先を指定してください").clone();
    let mut ids: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for f in &args[1..] {
        let file = std::fs::File::open(f).expect("データを開けません");
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            for e in v["entries"].as_array().into_iter().flatten() {
                if let Some(id) = e["horse_id"].as_str() {
                    if !id.is_empty() && seen.insert(id.to_string()) {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }
    let mut done = HashSet::new();
    if let Ok(f) = std::fs::File::open(&out) {
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(id) = v["horse_id"].as_str() {
                    done.insert(id.to_string());
                }
            }
        }
    }
    let todo: Vec<&String> = ids.iter().filter(|id| !done.contains(*id)).collect();
    eprintln!("馬 {} 頭 / 取得済み {} / 残り {}", ids.len(), done.len(), todo.len());

    let workers: usize = std::env::var("WORKERS").ok().and_then(|s| s.parse().ok()).unwrap_or(1).clamp(1, 5);
    eprintln!("並列 {}", workers);
    let file = std::sync::Arc::new(std::sync::Mutex::new(
        std::fs::OpenOptions::new().create(true).append(true).open(&out).expect("出力先を開けません"),
    ));
    let queue = std::sync::Arc::new(std::sync::Mutex::new(todo.into_iter().cloned().collect::<Vec<String>>()));
    let fails = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(done.len()));
    let total = ids.len();
    let mut set = tokio::task::JoinSet::new();
    for w in 0..workers {
        let (file, queue, fails, count) = (file.clone(), queue.clone(), fails.clone(), count.clone());
        set.spawn(async move {
            use std::sync::atomic::Ordering;
            tokio::time::sleep(std::time::Duration::from_millis(700 * w as u64)).await;
            loop {
                if fails.load(Ordering::Relaxed) >= 5 {
                    break;
                }
                let Some(id) = queue.lock().unwrap().pop() else { break };
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                match scraper::fetch_horse_results(&id).await {
                    Ok(runs) => {
                        fails.store(0, Ordering::Relaxed);
                        let line = serde_json::json!({ "horse_id": id, "runs": runs });
                        writeln!(file.lock().unwrap(), "{}", line).unwrap();
                        let n = count.fetch_add(1, Ordering::Relaxed) + 1;
                        if n % 50 == 0 {
                            eprintln!("{} / {} ({} 走)", n, total, runs.len());
                        }
                    }
                    Err(e) => {
                        let f = fails.fetch_add(1, Ordering::Relaxed) + 1;
                        eprintln!("{} 取得失敗 ({}回連続): {}", id, f, e);
                        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                    }
                }
            }
        });
    }
    while set.join_next().await.is_some() {}
    if fails.load(std::sync::atomic::Ordering::Relaxed) >= 5 {
        eprintln!("失敗が続いたので中断しました");
        std::process::exit(2);
    }
    eprintln!("完了");
}
