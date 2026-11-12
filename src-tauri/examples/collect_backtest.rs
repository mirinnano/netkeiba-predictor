//! バックテスト用に、確定済みの JRA レースを netkeiba から集めて JSON Lines に保存する。
//!
//!   cargo run --example collect_backtest -- <出力先.jsonl> <年> [1か月あたりの開催日数] [並列数]
//!
//! 1. 各月の 15 日前後の土日からレース一覧を順に集める（1.5 秒間隔）。
//! 2. レースごとの取得を並列数ぶん同時に走らせる。各ワーカーはレースごとに 1.5 秒待つ。
//! 失敗が続いたら（ブロックされた可能性があるので）止まる。途中で止めても、保存済みのレースは次回飛ばす。
use netkeiba_predictor_lib::scraper;
use serde_json::json;
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const MAX_CONSECUTIVE_FAILURES: usize = 5;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).expect("出力先を指定してください").clone();
    let year: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2024);
    let per_month: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let workers: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1).clamp(1, 8);

    let mut done: HashSet<String> = HashSet::new();
    if let Ok(f) = std::fs::File::open(&out) {
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(id) = v["race_info"]["race_id"].as_str() {
                    done.insert(id.to_string());
                }
            }
        }
    }
    eprintln!("{}: 既存 {} レース / 並列 {}", year, done.len(), workers);

    // 1. 対象レースを集める
    let mut queue: Vec<(String, String)> = Vec::new(); // (開催日, race_id)
    for month in 1..=12u32 {
        let mut candidates: Vec<String> = Vec::new();
        for off in 0..14i32 {
            for sign in [1i32, -1] {
                let day = 15 + sign * off;
                if !(1..=28).contains(&day) || (off == 0 && sign == -1) {
                    continue;
                }
                let Some(date) = chrono::NaiveDate::from_ymd_opt(year as i32, month, day as u32) else { continue };
                use chrono::Datelike;
                if matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
                    candidates.push(date.format("%Y%m%d").to_string());
                }
            }
        }
        let mut used = 0;
        for date in candidates {
            if used >= per_month {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let venues = match scraper::fetch_race_list(&date, false).await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{} レース一覧取得失敗: {}", date, e);
                    continue;
                }
            };
            if venues.is_empty() {
                continue;
            }
            used += 1;
            for v in venues {
                for r in v.races {
                    if !done.contains(&r.race_id) && r.surface != "障" {
                        queue.push((date.clone(), r.race_id));
                    }
                }
            }
        }
    }
    eprintln!("{}: 取得対象 {} レース", year, queue.len());

    // 2. 並列で取得する
    let file = Arc::new(Mutex::new(
        std::fs::OpenOptions::new().create(true).append(true).open(&out).expect("出力先を開けません"),
    ));
    let queue = Arc::new(Mutex::new(queue));
    let saved = Arc::new(AtomicUsize::new(done.len()));
    let fails = Arc::new(AtomicUsize::new(0));
    let mut set = tokio::task::JoinSet::new();
    for w in 0..workers {
        let (file, queue, saved, fails) = (file.clone(), queue.clone(), saved.clone(), fails.clone());
        set.spawn(async move {
            // 各ワーカーの開始をずらす
            tokio::time::sleep(std::time::Duration::from_millis(300 * w as u64)).await;
            loop {
                if fails.load(Ordering::Relaxed) >= MAX_CONSECUTIVE_FAILURES {
                    break;
                }
                let Some((date, id)) = queue.lock().unwrap().pop() else { break };
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                match scraper::scrape_race_data(&id).await {
                    Ok((info, entries, payouts, combo)) => {
                        fails.store(0, Ordering::Relaxed);
                        if !info.is_finished || info.surface == "障" {
                            continue;
                        }
                        let line = json!({
                            "race_info": info,
                            "entries": entries,
                            "payouts": payouts,
                            "combo_odds": combo,
                        });
                        writeln!(file.lock().unwrap(), "{}", line).unwrap();
                        let n = saved.fetch_add(1, Ordering::Relaxed) + 1;
                        eprintln!("{} {} ({}件目)", date, id, n);
                    }
                    Err(e) => {
                        let f = fails.fetch_add(1, Ordering::Relaxed) + 1;
                        eprintln!("{} 取得失敗 ({}回連続): {}", id, f, e);
                        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    }
                }
            }
        });
    }
    while set.join_next().await.is_some() {}
    if fails.load(Ordering::Relaxed) >= MAX_CONSECUTIVE_FAILURES {
        eprintln!("{}: 失敗が {} 回続いたので中断しました", year, MAX_CONSECUTIVE_FAILURES);
        std::process::exit(2);
    }
    eprintln!("{}: 完了 {} レース", year, saved.load(Ordering::Relaxed));
}
