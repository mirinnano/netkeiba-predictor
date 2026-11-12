//! JvDump の出力（RACE/RA.txt・SE.txt）から、重複を除いた全レースの JSON Lines を作り、
//! netkeiba から集めたデータと突き合わせて読み取りの正しさを確かめる。
//!
//!   cargo run --release --example jv_build -- <jvdata/out> <出力.jsonl> [netkeiba races.jsonl ...]
use netkeiba_predictor_lib::jvdata::load_races;
use std::collections::HashMap;
use std::io::{BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::PathBuf::from(&args[0]);
    let out = &args[1];
    let races = load_races(&root.join("RACE")).expect("読み込めません");
    let runners: usize = races.iter().map(|r| r.runners.len()).sum();
    let finals = races.iter().filter(|r| r.kubun == '7').count();
    let flat = races.iter().filter(|r| r.surface == "芝" || r.surface == "ダ").count();
    println!(
        "レース {}（確定成績 {}、平地 {}）/ 出走 {} / 期間 {} 〜 {}",
        races.len(),
        finals,
        flat,
        runners,
        races.first().map(|r| r.date.as_str()).unwrap_or(""),
        races.last().map(|r| r.date.as_str()).unwrap_or("")
    );
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    for r in &races {
        writeln!(f, "{}", serde_json::to_string(r).unwrap()).unwrap();
    }
    println!("書き出し: {}", out);

    // netkeiba のデータと突き合わせる（着順・タイム・単勝オッズ・枠・斤量・上がり3F）
    if args.len() > 2 {
        let by_id: HashMap<&str, &netkeiba_predictor_lib::jvdata::JvRace> =
            races.iter().map(|r| (r.race_id.as_str(), r)).collect();
        let (mut n, mut found, mut ok_rank, mut ok_time, mut ok_odds, mut ok_waku, mut ok_cw, mut cmp_l3, mut ok_l3, mut ok_surf) =
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut examples: Vec<String> = Vec::new();
        for p in &args[2..] {
            let file = std::fs::File::open(p).unwrap();
            for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = v["race_info"]["race_id"].as_str().unwrap_or("");
                let Some(jr) = by_id.get(id) else { continue };
                if v["race_info"]["surface"].as_str() == Some(jr.surface.as_str()) {
                    ok_surf += 1;
                }
                found += 1;
                for e in v["entries"].as_array().unwrap() {
                    if e["is_scratched"].as_bool() == Some(true) {
                        continue;
                    }
                    let u = e["umaban"].as_u64().unwrap() as u32;
                    let Some(h) = jr.runners.iter().find(|h| h.umaban == u) else { continue };
                    n += 1;
                    let nk_rank = e["actual_rank"].as_u64().map(|x| x as u32);
                    if nk_rank == h.rank {
                        ok_rank += 1;
                    } else if examples.len() < 5 {
                        examples.push(format!("{} {}番 着順 netkeiba={:?} JV={:?}", id, u, nk_rank, h.rank));
                    }
                    let nk_time = e["actual_time"].as_str().and_then(|t| {
                        let (m, s) = t.split_once(':').unwrap_or(("0", t));
                        Some(m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?)
                    });
                    if match (nk_time, h.time_sec) {
                        (Some(a), Some(b)) => (a - b).abs() < 0.05,
                        (None, None) => true,
                        _ => false,
                    } {
                        ok_time += 1;
                    }
                    if match (e["win_odds"].as_f64(), h.win_odds) {
                        (Some(a), Some(b)) => (a - b).abs() < 0.05,
                        _ => false,
                    } {
                        ok_odds += 1;
                    }
                    if e["waku"].as_u64() == Some(h.waku as u64) {
                        ok_waku += 1;
                    }
                    if match (e["carried_weight"].as_f64(), h.carried_weight) {
                        (Some(a), Some(b)) => (a - b).abs() < 0.05,
                        _ => false,
                    } {
                        ok_cw += 1;
                    }
                    if let (Some(a), Some(b)) = (e["actual_last_3f"].as_f64(), h.last3f) {
                        cmp_l3 += 1;
                        if (a - b).abs() < 0.05 {
                            ok_l3 += 1;
                        }
                    }
                }
            }
        }
        let pct = |a: usize| a as f64 / n.max(1) as f64 * 100.0;
        println!("\n[netkeiba との突き合わせ] 一致したレース {} / 出走 {}", found, n);
        println!("  芝ダ {:.1}%（レース単位）", ok_surf as f64 / found.max(1) as f64 * 100.0);
        println!("  着順 {:.2}% / タイム {:.2}% / 単勝オッズ {:.2}% / 枠 {:.2}% / 斤量 {:.2}%", pct(ok_rank), pct(ok_time), pct(ok_odds), pct(ok_waku), pct(ok_cw));
        println!("  上がり3F {:.2}%（比べられた {} 頭）", ok_l3 as f64 / cmp_l3.max(1) as f64 * 100.0, cmp_l3);
        for e in &examples {
            println!("  不一致の例: {}", e);
        }
        if let Some(jc) = by_id.get("202305050812") {
            println!("\n[2023 ジャパンC] {} {}m {} {}頭 前3F={:?} 後3F={:?}", jc.race_name, jc.distance, jc.surface, jc.runners_count, jc.first3f, jc.last3f);
            for h in jc.runners.iter().filter(|h| h.rank.map(|r| r <= 3).unwrap_or(false)) {
                println!(
                    "  {}着 {}番 {} {} 斤量{:?} 単勝{:?} {}人気 タイム{:?} 上がり{:?} 通過{:?} マイニング予想{:?}位",
                    h.rank.unwrap(), h.umaban, h.name, h.jockey_name, h.carried_weight, h.win_odds,
                    h.popularity.unwrap_or(0), h.time_sec, h.last3f, h.corners, h.mining_rank
                );
            }
        }
    }
}
