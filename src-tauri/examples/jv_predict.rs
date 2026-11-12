//! DB にある出馬表（JRA-VAN）で1レースを予測する（netkeiba を使わない確認用）。
//!
//!   cargo run --release --example jv_predict -- <keiba.db> <race_id>
use netkeiba_predictor_lib::{jvmodel, jvstore};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let conn = jvstore::open(std::path::Path::new(&args[0])).unwrap();
    let race = jvstore::race_by_id(&conn, &args[1]).unwrap().expect("DB にそのレースがありません");
    let date: u32 = race.date.parse().unwrap();
    let t = std::time::Instant::now();
    let engine = jvmodel::build_engine(&conn, date).unwrap();
    println!("Engine を {:.1} 秒で作成: {}", t.elapsed().as_secs_f64(), engine.summary());
    let card = jvmodel::card_from_jv(&race);
    let x = engine.features(&card);
    let odds: Vec<Option<f64>> = race.runners.iter().filter(|h| jvmodel::started(h)).map(|h| h.win_odds).collect();
    let (p, with_market) = jvmodel::win_probs(&x, &odds);
    println!("{} {} {}m {}頭（データ区分 {}）/ {}", race.date, race.race_id, race.distance, card.runners.len(), race.kubun, if with_market { "市場＋モデル" } else { "モデルのみ（オッズなし）" });
    let mut idx: Vec<usize> = (0..p.len()).collect();
    idx.sort_by(|&a, &b| p[b].partial_cmp(&p[a]).unwrap());
    let names: Vec<&String> = race.runners.iter().filter(|h| jvmodel::started(h)).map(|h| &h.name).collect();
    for &i in idx.iter().take(6) {
        let h = &card.runners[i];
        println!("  {:>2}番 {:<18} 勝率 {:>5.1}%  過去走 {}", h.umaban, names[i], p[i] * 100.0, if engine.has_history(&h.horse_id, date) { "あり" } else { "なし" });
    }
    println!("  勝率の合計 {:.4}", p.iter().sum::<f64>());
}
