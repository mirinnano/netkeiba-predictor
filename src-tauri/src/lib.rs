pub mod db;
pub mod models;
pub mod predictor;
pub mod scraper;

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
    input: String,
    weights: Option<ScoringWeights>,
    budget: Option<u32>,
    strategy: Option<String>,
) -> Result<RacePredictionResult, String> {
    let (race_info, entries, official_payouts) = scraper::scrape_race_data(&input).await?;
    let w = weights.unwrap_or_default();
    let b = budget.unwrap_or(3000);
    let s = strategy.unwrap_or_else(|| "balance".to_string());

    Ok(predictor::run_prediction(
        race_info,
        entries,
        official_payouts,
        w,
        b,
        s,
        String::new(),
    ))
}

#[tauri::command]
fn recalculate_prediction(
    current_result: RacePredictionResult,
    weights: ScoringWeights,
    budget: u32,
    strategy: String,
    memo: Option<String>,
) -> Result<RacePredictionResult, String> {
    let entries: Vec<HorseEntry> = current_result
        .predictions
        .into_iter()
        .map(|p| p.entry)
        .collect();

    let updated = predictor::run_prediction(
        current_result.race_info,
        entries,
        current_result.official_payouts,
        weights,
        budget,
        strategy,
        memo.unwrap_or(current_result.memo),
    );
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

#[tauri::command]
async fn verify_saved_race_result(
    app: tauri::AppHandle,
    race_id: String,
) -> Result<RacePredictionResult, String> {
    let db_path = resolve_db_path(&app);
    let saved = db::load_prediction(&db_path, &race_id)?;
    let (race_info, entries, official_payouts) = scraper::scrape_race_data(&race_id).await?;
    let updated = predictor::run_prediction(
        race_info,
        entries,
        official_payouts,
        saved.weights,
        saved.budget,
        saved.strategy,
        saved.memo,
    );
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_live_scrape_and_predict() {
        let (race_info, entries, payouts) = scraper::scrape_race_data("202305050812")
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
        );

        assert_eq!(result.predictions.len(), 18);
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

        let (race_info, entries, payouts) = scraper::scrape_race_data(first_race_id)
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

