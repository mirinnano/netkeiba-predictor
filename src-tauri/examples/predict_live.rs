//! アプリの analyze_race と同じ経路で、開催日のレースを予想する（netkeiba の出馬表 → JRA-VAN モデル → 買い目）。
//!
//!   cargo run --release --example predict_live -- <keiba.db> <開催日 YYYYMMDD> <レース名の一部> [--json 出力先]
use netkeiba_predictor_lib::{jvapp, models, predictor, scraper};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = std::path::PathBuf::from(&args[0]);
    let (date, name) = (&args[1], &args[2]);
    let venues = scraper::fetch_race_list(date, false).await.expect("レース一覧を取れません");
    let race = venues
        .iter()
        .flat_map(|v| v.races.iter())
        .find(|r| r.title.contains(name.as_str()))
        .expect("そのレースが見つかりません");
    println!("{} {} {}", race.race_id, race.title, race.start_time);
    let (info, entries, _pay, combo) = scraper::scrape_race_data(&race.race_id).await.expect("出馬表を取れません");
    let none = std::collections::HashMap::new();
    let jv = jvapp::jv_probs(&db, &info, &entries, &none).expect("JRA-VAN モデルでエラー");
    let r = predictor::run_prediction_with(info, entries, Vec::new(), models::ScoringWeights::default(), 3000, "balance".into(), String::new(), combo, jv.as_ref());
    println!(
        "{} {} {}{}m {} / モデル: {} / オッズあり: {}",
        r.race_info.date_str,
        r.race_info.venue,
        r.race_info.surface,
        r.race_info.distance,
        r.race_info.track_cond,
        r.model_source,
        r.predictions.iter().any(|p| p.entry.win_odds.is_some())
    );
    for p in r.predictions.iter().filter(|p| !p.entry.is_scratched) {
        println!(
            "  {:<2} {:>2}番 {:<14} {:<4} {:<6} 指数 {:>5.1} 勝率 {:>5.1}% 3着内 {:>5.1}% 単勝 {:>6} 期待値 {:.2} | 能力 {:.0} 時計 {:.0} 適性 {:.0} 騎手 {:.0} 展開 {:.0} 調教 {}",
            p.mark,
            p.entry.umaban,
            p.entry.horse_name,
            p.entry.sex_age,
            p.entry.jockey_name,
            p.total_index,
            p.win_prob * 100.0,
            p.top3_prob * 100.0,
            p.entry.win_odds.map(|o| format!("{:.1}", o)).unwrap_or("-".into()),
            p.expected_value,
            p.ability_score,
            p.speed_score,
            p.aptitude_score,
            p.jockey_frame_score,
            p.pace_score,
            p.training_score.map(|t| format!("{:.0}", t)).unwrap_or("-".into())
        );
    }
    println!("  展開: {} / {}", r.pace_analysis.predicted_pace, r.pace_analysis.commentary);
    println!("  買い目（{} 円）:", r.total_bet);
    for t in r.recommendations.iter().filter(|t| t.suggested_amount > 0) {
        println!(
            "    {} {} {} 円 的中 {:.1}% オッズ {}{:.1} 期待値 {:.2}{}",
            t.bet_type,
            t.combination_text,
            t.suggested_amount,
            t.est_hit_prob * 100.0,
            if t.odds_is_estimated { "推定" } else { "" },
            t.est_odds,
            t.expected_value,
            if t.recommended { " おすすめ" } else { "" }
        );
    }
    if let Some(i) = args.iter().position(|a| a == "--json") {
        std::fs::write(&args[i + 1], serde_json::to_string(&r).unwrap()).unwrap();
    }
}
