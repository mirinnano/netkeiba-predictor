//! netkeiba の結果ページから作ったレースを、DB の JRA-VAN の確定成績と項目ごとに照合する。
//!
//!   cargo run --release --example nk_fill_check -- <keiba.db> <開催日 YYYYMMDD>...
use netkeiba_predictor_lib::{jvstore, scraper};
use std::collections::BTreeMap;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let conn = jvstore::open(std::path::Path::new(&args[0])).unwrap();
    let client = scraper::build_client().unwrap();
    let mut stat: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
    let mut bump = |k: &'static str, ok: bool| {
        let e = stat.entry(k).or_default();
        e.1 += 1;
        if ok {
            e.0 += 1;
        }
    };
    let close = |a: Option<f64>, b: Option<f64>, tol: f64| match (a, b) {
        (Some(x), Some(y)) => (x - y).abs() <= tol,
        (None, None) => true,
        _ => false,
    };
    let mut examples: Vec<String> = Vec::new();
    for date in &args[1..] {
        let venues = scraper::fetch_race_list(date, false).await.unwrap_or_default();
        for r in venues.iter().flat_map(|v| v.races.iter()) {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            let Some(nk) = scraper::fetch_result_as_jvrace(&client, &r.race_id, date).await else { continue };
            let Some(jv) = jvstore::race_by_id(&conn, &r.race_id).ok().flatten() else { continue };
            if jv.kubun != '7' {
                continue;
            }
            bump("レース: 距離", nk.distance == jv.distance);
            bump("レース: 馬場(芝ダ)", nk.surface == jv.surface);
            bump("レース: トラックコード", nk.track_code == jv.track_code);
            if nk.track_code != jv.track_code && examples.len() < 6 {
                examples.push(format!("{} {} nk={} jv={}", r.race_id, r.title, nk.track_code, jv.track_code));
            }
            bump("レース: 条件コード", nk.condition_min == jv.condition_min || (nk.condition_min == "703" && ["701", "702", "703"].contains(&jv.condition_min.as_str())));
            bump("レース: グレード", nk.grade == jv.grade || (nk.grade.is_empty() && !["A", "B", "C", "L"].contains(&jv.grade.as_str())));
            bump("レース: 馬場状態", (if jv.surface == "芝" { &jv.turf_cond } else { &jv.dirt_cond }) == (if nk.surface == "芝" { &nk.turf_cond } else { &nk.dirt_cond }));
            bump("レース: 前3F", close(nk.first3f, jv.first3f, 0.05));
            let (jg, ng) = (if jv.surface == "芝" { &jv.turf_cond } else { &jv.dirt_cond }, if nk.surface == "芝" { &nk.turf_cond } else { &nk.dirt_cond });
            if jg != ng && examples.len() < 8 {
                examples.push(format!("馬場状態 {} {} jv={} nk={}", r.race_id, jv.surface, jg, ng));
            }
            if !close(nk.first3f, jv.first3f, 0.05) && examples.len() < 12 {
                examples.push(format!("前3F {} {}m nk={:?} jv={:?} laps={:?}", r.race_id, jv.distance, nk.first3f, jv.first3f, &nk.laps[..nk.laps.len().min(4)]));
            }

            bump("レース: 後3F", close(nk.last3f, jv.last3f, 0.05));
            for h in &jv.runners {
                let Some(n) = nk.runners.iter().find(|x| x.horse_id == h.horse_id) else {
                    bump("馬: 見つかる", false);
                    continue;
                };
                bump("馬: 見つかる", true);
                bump("馬: 馬番", n.umaban == h.umaban);
                bump("馬: 着順", n.rank == h.rank);
                bump("馬: 走破タイム", close(n.time_sec, h.time_sec, 0.05));
                bump("馬: 上がり3F", close(n.last3f, h.last3f, 0.05));
                bump("馬: 4角順位", n.corners[3] == h.corners[3]);
                bump("馬: 人気", n.popularity == h.popularity);
                bump("馬: 単勝オッズ", close(n.win_odds, h.win_odds, 0.05));
                bump("馬: 騎手コード", n.jockey == h.jockey);
                bump("馬: 調教師コード", n.trainer == h.trainer);
                bump("馬: 斤量", close(n.carried_weight, h.carried_weight, 0.05));
                bump("馬: 馬体重", n.body_weight == h.body_weight);
                bump("馬: 増減", n.weight_diff == h.weight_diff);
                if n.weight_diff != h.weight_diff && examples.len() < 20 {
                    examples.push(format!("増減 {} {} nk={:?} jv={:?} 体重 nk={:?} jv={:?}", r.race_id, h.umaban, n.weight_diff, h.weight_diff, n.body_weight, h.body_weight));
                }
                bump("馬: 馬齢", n.age == h.age);
                bump("馬: 性別", n.sex == h.sex);
                bump("馬: タイム差", close(n.time_diff, h.time_diff, 0.05));
                bump("馬: 脚質", n.style == h.style);
                bump("馬: 出走/除外", (n.abnormal == "1" || n.abnormal == "2") == (h.abnormal == "1" || h.abnormal == "2"));
            }
        }
    }
    for (k, (ok, n)) in &stat {
        println!("{:<16} {:>5}/{:<5} {:>6.1}%", k, ok, n, *ok as f64 / (*n).max(1) as f64 * 100.0);
    }
    for e in examples {
        println!("  {}", e);
    }
}
