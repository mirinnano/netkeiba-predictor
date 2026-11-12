//! 締め切り前のオッズを記録する。バックテストを「実際に買える時点のオッズ」でやり直すための材料。
//!
//!   cargo run --release --example record_odds -- <出力先.jsonl> <開催日 YYYYMMDD> [発走何分前,...]
//!
//! 既定では発走 30・10・3 分前に、その日の JRA 全レースの単勝・複勝と、馬連・ワイド・3連複のオッズを取る。
//! 開催日の朝に起動して、最終レースが終わるまで動かしておく。1レース1時点あたり4リクエスト。
use chrono::{Local, NaiveTime, TimeZone};
use netkeiba_predictor_lib::scraper;
use serde_json::json;
use std::io::Write;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().expect("出力先を指定してください").clone();
    let date = args.get(1).expect("開催日を指定してください").clone();
    let offsets: Vec<i64> = args
        .get(2)
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![30, 10, 3]);

    let venues = scraper::fetch_race_list(&date, false).await.expect("レース一覧を取得できません");
    let day = chrono::NaiveDate::parse_from_str(&date, "%Y%m%d").expect("日付の形式が違います");
    // (取得時刻, race_id, 発走何分前)
    let mut jobs: Vec<(chrono::DateTime<Local>, String, i64)> = Vec::new();
    for v in &venues {
        for r in &v.races {
            let Ok(t) = NaiveTime::parse_from_str(&r.start_time, "%H:%M") else { continue };
            let Some(start) = Local.from_local_datetime(&day.and_time(t)).single() else { continue };
            for &m in &offsets {
                jobs.push((start - chrono::Duration::minutes(m), r.race_id.clone(), m));
            }
        }
    }
    jobs.sort();
    let now = Local::now();
    jobs.retain(|(at, _, _)| *at > now - chrono::Duration::minutes(1));
    eprintln!("{}: {} レース分、{} 回の記録を予定", date, venues.iter().map(|v| v.races.len()).sum::<usize>(), jobs.len());

    let client = scraper::build_client().unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&out).expect("出力先を開けません");
    let mut fails = 0;
    for (at, race_id, m) in jobs {
        let wait = (at - Local::now()).num_milliseconds();
        if wait > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(wait as u64)).await;
        }
        let win = scraper::fetch_jra_odds(&client, &race_id).await;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let combo = scraper::fetch_jra_combo_odds(&client, &race_id).await;
        if win.is_empty() {
            fails += 1;
            eprintln!("{} {}分前: オッズが取れませんでした（{}回連続）", race_id, m, fails);
            if fails >= 8 {
                eprintln!("失敗が続いたので中断しました");
                std::process::exit(2);
            }
            continue;
        }
        fails = 0;
        let win_map: serde_json::Map<String, serde_json::Value> = win
            .iter()
            .map(|(k, (w, pmin, pmax, pop))| {
                (k.to_string(), json!({ "win": w, "place_min": pmin, "place_max": pmax, "pop": pop }))
            })
            .collect();
        let line = json!({
            "race_id": race_id,
            "minutes_before": m,
            "captured_at": Local::now().to_rfc3339(),
            "win": win_map,
            "combo_odds": combo,
        });
        writeln!(file, "{}", line).unwrap();
        file.flush().ok();
        eprintln!("{} {}分前: {} 頭", race_id, m, win.len());
    }
    eprintln!("完了");
}
