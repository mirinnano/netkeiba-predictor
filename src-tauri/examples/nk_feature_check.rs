//! netkeiba で補った成績が、特徴量と勝率をどれだけ変えるかを確かめる。
//! DB の写しから指定日の JRA-VAN の確定成績を消して netkeiba の結果で埋め、元の DB と同じ出馬表の特徴量・勝率を比べる。
//!
//!   cargo run --release --example nk_feature_check -- <keiba.db> <出馬表の日 YYYYMMDD> <補う日 YYYYMMDD>...
use netkeiba_predictor_lib::{jvmodel, jvstore, scraper};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let src = std::path::PathBuf::from(&args[0]);
    let card_day: u32 = args[1].parse().unwrap();
    let fill_days = &args[2..];
    let tmp = std::env::temp_dir().join("keiba_nkcheck.db");
    let _ = std::fs::remove_file(&tmp);
    rusqlite::Connection::open(&src).unwrap().execute(&format!("VACUUM INTO '{}'", tmp.display()), []).unwrap();
    let mut filled = jvstore::open(&tmp).unwrap();
    let client = scraper::build_client().unwrap();
    let mut n_races = 0;
    for d in fill_days {
        filled.execute("DELETE FROM runners WHERE race_key IN (SELECT race_key FROM races WHERE date = ?1)", [d]).unwrap();
        filled.execute("DELETE FROM races WHERE date = ?1", [d]).unwrap();
        for r in scraper::fetch_race_list(d, false).await.unwrap_or_default().iter().flat_map(|v| v.races.iter()) {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            if let Some(race) = scraper::fetch_result_as_jvrace(&client, &r.race_id, d).await {
                jvstore::store_netkeiba_race(&mut filled, &race).unwrap();
                n_races += 1;
            }
        }
    }
    println!("netkeiba で補ったレース {}", n_races);
    let orig = jvstore::open(&src).unwrap();
    let e_orig = jvmodel::build_engine(&orig, card_day).unwrap();
    let e_fill = jvmodel::build_engine(&filled, card_day).unwrap();
    let cards: Vec<_> = jvstore::load_races(&orig, &card_day.to_string(), &(card_day + 1).to_string()).unwrap().into_iter().filter(|r| r.surface == "芝" || r.surface == "ダ").collect();
    let k = jvmodel::ADOPTED.len();
    let (mut diff_sum, mut cnt) = (vec![0.0f64; k], 0usize);
    let (mut max_dp, mut sum_dp, mut horses) = (0.0f64, 0.0f64, 0usize);
    for r in &cards {
        let card = jvmodel::card_from_jv(r);
        if card.runners.len() < 2 {
            continue;
        }
        let a = e_orig.features(&card);
        let b = e_fill.features(&card);
        for (ra, rb) in a.iter().zip(&b) {
            for (j, &f) in jvmodel::ADOPTED.iter().enumerate() {
                diff_sum[j] += (ra[f] - rb[f]).abs();
            }
            cnt += 1;
        }
        let no_odds: Vec<Option<f64>> = vec![None; card.runners.len()];
        let (pa, _) = jvmodel::win_probs(&a, &no_odds);
        let (pb, _) = jvmodel::win_probs(&b, &no_odds);
        for (x, y) in pa.iter().zip(&pb) {
            let d = (x - y).abs();
            max_dp = max_dp.max(d);
            sum_dp += d;
            horses += 1;
        }
    }
    println!("比べた出馬表 {} レース・{} 頭", cards.len(), horses);
    println!("勝率の差（オッズなしのモデル）: 平均 {:.4} / 最大 {:.4}", sum_dp / horses.max(1) as f64, max_dp);
    println!("特徴量の差（標準化後の平均絶対差、大きい順）:");
    let mut v: Vec<(usize, f64)> = diff_sum.iter().enumerate().map(|(j, s)| (j, s / cnt.max(1) as f64)).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for (j, d) in v.iter().take(8) {
        println!("  {:<14} {:.3}", jvmodel::NAMES[jvmodel::ADOPTED[*j]], d);
    }
}
