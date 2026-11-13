pub mod db;
pub mod jv_coef;
pub mod jvapp;
pub mod jvdata;
pub mod jvmodel;
pub mod jvstore;
pub mod models;
pub mod predictor;
pub mod scraper;
pub mod std_times;

use models::{
    HorseEntry, KaisaiCalendarResponse, RacePredictionResult, RoiDashboardStats, ScoringWeights,
    VenueRaces,
};
use std::path::PathBuf;
use tauri::Manager;

fn resolve_db_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("keibascope_predictions.db")
}

#[tauri::command]
async fn get_kaisai_calendar(
    target_date: Option<String>,
    is_nar: Option<bool>,
) -> Result<KaisaiCalendarResponse, String> {
    scraper::fetch_kaisai_dates(target_date, is_nar.unwrap_or(false)).await
}

#[tauri::command]
async fn get_race_list_by_date(
    kaisai_date: String,
    is_nar: Option<bool>,
) -> Result<Vec<VenueRaces>, String> {
    scraper::fetch_race_list(&kaisai_date, is_nar.unwrap_or(false)).await
}

#[tauri::command]
async fn analyze_race(
    app: tauri::AppHandle,
    input: String,
    weights: Option<ScoringWeights>,
    budget: Option<u32>,
    strategy: Option<String>,
) -> Result<RacePredictionResult, String> {
    let db = app.path().app_data_dir().ok().and_then(|d| jvapp::db_path(&d));
    // 出馬表とオッズは netkeiba から。取れないときは DB（JRA-VAN の出馬表）にあればそれを使う
    let (race_info, entries, official_payouts, combo_odds) = match scraper::scrape_race_data(&input).await {
        Ok(x) => x,
        Err(e) => {
            let (race_id, _) = scraper::extract_race_id(&input)?;
            match db.as_deref().and_then(|p| jvapp::race_from_db(p, &race_id)) {
                Some((info, entries)) => (info, entries, Vec::new(), models::ComboOdds::default()),
                None => return Err(e),
            }
        }
    };
    let w = weights.unwrap_or_default();
    let b = budget.unwrap_or(3000);
    let s = strategy.unwrap_or_else(|| "balance".to_string());

    // DB があれば JRA-VAN モデルの勝率を使う（数秒かかるので別スレッドで）
    let jv = match db {
        Some(p) => {
            let (info, ents) = (race_info.clone(), entries.clone());
            tokio::task::spawn_blocking(move || jvapp::jv_probs(&p, &info, &ents))
                .await
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|e| {
                    eprintln!("JRA-VAN モデルを使えませんでした: {}", e);
                    None
                })
        }
        None => None,
    };

    Ok(predictor::run_prediction_with(
        race_info,
        entries,
        official_payouts,
        w,
        b,
        s,
        String::new(),
        combo_odds,
        jv.as_ref(),
    ))
}

/// JvDump の出力フォルダ（out）を、アプリのデータフォルダの keiba.db に取り込む（前回の続きから）
#[tauri::command]
async fn jv_import(app: tauri::AppHandle, dir: String) -> Result<Vec<jvstore::IngestReport>, String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let db = jvapp::db_path(&data).unwrap_or_else(|| data.join("keiba.db"));
    let out = std::path::PathBuf::from(dir.trim());
    if !out.join("RACE").exists() {
        return Err(format!("JvDump の出力（RACE フォルダ）が見つかりません: {}", out.display()));
    }
    tokio::task::spawn_blocking(move || {
        let mut conn = jvstore::open(&db).map_err(|e| e.to_string())?;
        jvstore::ingest_dir(&mut conn, &out).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn jv_status(app: tauri::AppHandle) -> jvapp::JvStatus {
    let db = app.path().app_data_dir().ok().and_then(|d| jvapp::db_path(&d));
    jvapp::status(db.as_deref())
}

#[tauri::command]
fn recalculate_prediction(
    current_result: RacePredictionResult,
    weights: ScoringWeights,
    budget: u32,
    strategy: String,
    memo: Option<String>,
) -> Result<RacePredictionResult, String> {
    // 確定後に組み直した予想は結果を見た後のものなので、確定前フラグを落とす
    let pre_race = current_result.pre_race && !current_result.race_info.is_finished;
    let entries: Vec<HorseEntry> = current_result
        .predictions
        .into_iter()
        .map(|p| p.entry)
        .collect();

    // JRA-VAN モデルの勝率と寄与は再計算で変わらないので、保存されたものを使い続ける
    let jv: Option<models::JvModelOut> = current_result.jv_probs.as_ref().map(|v| models::JvModelOut {
        probs: v.iter().cloned().collect(),
        contrib: current_result.jv_contrib.clone().unwrap_or_default().into_iter().collect(),
    });
    let mut updated = predictor::run_prediction_with(
        current_result.race_info,
        entries,
        current_result.official_payouts,
        weights,
        budget,
        strategy,
        memo.unwrap_or(current_result.memo),
        current_result.combo_odds,
        jv.as_ref(),
    );
    updated.pre_race = pre_race;
    Ok(updated)
}

#[tauri::command]
fn save_prediction_record(
    app: tauri::AppHandle,
    result: RacePredictionResult,
) -> Result<RacePredictionResult, String> {
    let db_path = resolve_db_path(&app);
    db::save_prediction(&db_path, result)
}

#[tauri::command]
fn load_prediction_record(
    app: tauri::AppHandle,
    race_id: String,
) -> Result<RacePredictionResult, String> {
    let db_path = resolve_db_path(&app);
    db::load_prediction(&db_path, &race_id)
}

#[tauri::command]
fn delete_prediction_record(
    app: tauri::AppHandle,
    race_id: String,
) -> Result<RoiDashboardStats, String> {
    let db_path = resolve_db_path(&app);
    db::delete_prediction(&db_path, &race_id)?;
    db::get_dashboard_stats(&db_path)
}

#[tauri::command]
fn get_roi_dashboard(app: tauri::AppHandle) -> Result<RoiDashboardStats, String> {
    let db_path = resolve_db_path(&app);
    db::get_dashboard_stats(&db_path)
}

/// 保存済み予想の買い目を、確定した払戻と照合する。
/// 予想は作り直さない（作り直すと確定後のオッズで買い目が変わってしまうため）。
/// 未確定なら保存内容を変えずにそのまま返す。
#[tauri::command]
async fn verify_saved_race_result(
    app: tauri::AppHandle,
    race_id: String,
) -> Result<RacePredictionResult, String> {
    let db_path = resolve_db_path(&app);
    let saved = db::load_prediction(&db_path, &race_id)?;
    let (rows, payouts) = scraper::fetch_race_result(&race_id).await?;
    if rows.is_empty() {
        return Ok(saved);
    }
    let updated = predictor::apply_official_result(saved, &rows, payouts);
    db::save_prediction(&db_path, updated)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_kaisai_calendar,
            get_race_list_by_date,
            analyze_race,
            recalculate_prediction,
            save_prediction_record,
            load_prediction_record,
            delete_prediction_record,
            get_roi_dashboard,
            verify_saved_race_result,
            jv_status,
            jv_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_live_scrape_and_predict() {
        let (race_info, entries, payouts, combo) = scraper::scrape_race_data("202305050812")
            .await
            .expect("Failed to scrape 202305050812 from netkeiba");

        assert_eq!(race_info.race_id, "202305050812");
        assert!(race_info.race_name.contains("ジャパン"));
        assert_eq!(entries.len(), 18);
        assert!(race_info.is_finished);
        assert!(!payouts.is_empty());

        let result = predictor::run_prediction(
            race_info,
            entries,
            payouts,
            ScoringWeights::default(),
            3000,
            "balance".to_string(),
            "テスト保存".to_string(),
            combo,
        );

        assert_eq!(result.predictions.len(), 18);
        // JRA は組み合わせ馬券も実オッズが取れる
        assert!(result
            .recommendations
            .iter()
            .filter(|r| r.bet_type != "単勝")
            .all(|r| !r.odds_is_estimated));
        for r in &result.recommendations {
            println!(
                "{} {} p={:.3} odds={} EV={:.2} amount={} hit={:?}",
                r.bet_type, r.combination_text, r.est_hit_prob, r.est_odds, r.expected_value,
                r.suggested_amount, r.is_hit
            );
        }
        let honmei = result.predictions.first().unwrap();
        assert_eq!(honmei.mark, "◎");
        println!(
            "Top predicted: {} ({}) Index={:.1}, WinProb={:.1}%, Return={:?} yen (ROI={:?}%)",
            honmei.entry.horse_name,
            honmei.entry.umaban,
            honmei.total_index,
            honmei.win_prob * 100.0,
            result.total_return,
            result.roi_percent
        );
    }

    #[tokio::test]
    async fn test_nar_scrape_and_predict() {
        let cal = scraper::fetch_kaisai_dates(None, true)
            .await
            .expect("Failed to fetch NAR calendar");
        assert!(!cal.dates.is_empty());

        let venues = scraper::fetch_race_list(&cal.current_target, true)
            .await
            .expect("Failed to fetch NAR race list");
        assert!(!venues.is_empty());
        let first_race_id = &venues[0].races[0].race_id;

        let (race_info, entries, payouts, combo) = scraper::scrape_race_data(first_race_id)
            .await
            .expect("Failed to scrape NAR race");
        assert!(!entries.is_empty());

        let result = predictor::run_prediction(
            race_info,
            entries,
            payouts,
            ScoringWeights::default(),
            3000,
            "balance".to_string(),
            "NARテスト".to_string(),
            combo,
        );
        let honmei = result.predictions.first().unwrap();
        assert_eq!(honmei.mark, "◎");
        println!(
            "NAR Race {} ({}) Top predicted: {} ({}) Jockey={} Odds={:?}",
            result.race_info.venue,
            result.race_info.race_name,
            honmei.entry.horse_name,
            honmei.entry.umaban,
            honmei.entry.jockey_name,
            honmei.entry.win_odds
        );
    }
}

