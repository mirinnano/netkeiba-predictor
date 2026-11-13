//! アプリの analyze_race と同じ経路（netkeiba が使えないときの DB 出馬表 → JRA-VAN モデル → 買い目）を確かめる。
//!
//!   cargo run --release --example jv_app_check -- <keiba.db> <race_id>
use netkeiba_predictor_lib::{jvapp, models, predictor};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = std::path::PathBuf::from(&args[0]);
    let (info, entries) = jvapp::race_from_db(&db, &args[1]).expect("DB にそのレースがありません");
    let t = std::time::Instant::now();
    let jv = jvapp::jv_probs(&db, &info, &entries).expect("JRA-VAN モデルでエラー");
    let first = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let _ = jvapp::jv_probs(&db, &info, &entries).unwrap();
    println!("勝率の計算: 1回目 {:.1} 秒 / 2回目（Engine を使い回し）{:.2} 秒", first, t.elapsed().as_secs_f64());
    let r = predictor::run_prediction_with(
        info,
        entries,
        Vec::new(),
        models::ScoringWeights::default(),
        3000,
        "balance".into(),
        String::new(),
        models::ComboOdds::default(),
        jv.as_ref(),
    );
    if let Some(i) = args.iter().position(|a| a == "--json") {
        std::fs::write(&args[i + 1], serde_json::to_string(&r).unwrap()).expect("書き込めません");
    }
    println!("{} {} {} / モデル: {} / 確定: {}", r.race_info.date_str, r.race_info.race_name, r.race_info.source_url, r.model_source, r.race_info.is_finished);
    for p in r.predictions.iter().filter(|p| !p.entry.is_scratched).take(6) {
        println!(
            "  {:<2} {:>2}番 {:<16} 勝率 {:>5.1}% 3着内 {:>5.1}% 単勝 {:?} 期待値 {:.2}",
            p.mark, p.entry.umaban, p.entry.horse_name, p.win_prob * 100.0, p.top3_prob * 100.0, p.entry.win_odds, p.expected_value
        );
    }
    println!("  買い目 {} 点 / {} 円", r.recommendations.len(), r.total_bet);
    println!(
        "  指数の重み（自動）: {}",
        r.factor_shares.iter().map(|f| format!("{} {:.0}%", f.name, f.share * 100.0)).collect::<Vec<_>>().join(" / ")
    );
    if let Some(p) = r.predictions.iter().find(|p| p.rank_order == 1) {
        println!(
            "  1位 {}: 能力 {} 時計 {} 適性 {} 騎手 {} 展開 {} 調教 {:?}",
            p.entry.horse_name, p.ability_score, p.speed_score, p.aptitude_score, p.jockey_frame_score, p.pace_score, p.training_score
        );
    }
}
