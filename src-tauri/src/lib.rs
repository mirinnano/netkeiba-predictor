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
            let live = same_day_results(&p, &race_info.race_id).await;
            let (info, ents) = (race_info.clone(), entries.clone());
            tokio::task::spawn_blocking(move || jvapp::jv_probs(&p, &info, &ents, &live))
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

/// netkeiba から取った同じ日の先のレース結果（当日の馬場の偏り用）。確定したものだけを覚えて使い回す
static DAY_RESULTS: std::sync::Mutex<Option<std::collections::HashMap<String, (String, Vec<jvmodel::DayRow>)>>> = std::sync::Mutex::new(None);

/// 同じ日の先のレースのうち、DB に確定成績がないものを netkeiba から取る（1 秒に 1 件まで。取れなければ使わない）
async fn same_day_results(db: &std::path::Path, race_id: &str) -> std::collections::HashMap<String, (String, Vec<jvmodel::DayRow>)> {
    // JRA（場コード 01〜10）だけ
    let jra = race_id.get(4..6).and_then(|v| v.parse::<u32>().ok()).map(|v| (1..=10).contains(&v)).unwrap_or(false);
    if !jra {
        return Default::default();
    }
    let missing = jvapp::missing_earlier_races(db, race_id);
    let cached: Vec<String> = {
        let g = DAY_RESULTS.lock().unwrap();
        g.as_ref().map(|m| missing.iter().filter(|id| m.contains_key(*id)).cloned().collect()).unwrap_or_default()
    };
    if let Ok(client) = scraper::build_client() {
        for id in missing.iter().filter(|id| !cached.contains(id)) {
            if let Some((surface, rows)) = scraper::fetch_result_for_bias(&client, id).await {
                let v: Vec<jvmodel::DayRow> = rows.iter().map(|r| jvmodel::DayRow { umaban: r.umaban, rank: r.rank, corner_last: r.corner_last }).collect();
                DAY_RESULTS.lock().unwrap().get_or_insert_with(Default::default).insert(id.clone(), (surface, v));
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
    }
    let g = DAY_RESULTS.lock().unwrap();
    g.as_ref().map(|m| missing.iter().filter_map(|id| m.get(id).map(|v| (id.clone(), v.clone()))).collect()).unwrap_or_default()
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
        .setup(|_app| {
            // Android: 転送された DB があれば取り込む
            #[cfg(target_os = "android")]
            if let Ok(d) = _app.path().app_data_dir() {
                jvapp::import_pushed_db(&d);
            }
            // デスクトップだけ: 起動時にウィンドウを前に出す（Android にはウィンドウ操作がない）
            #[cfg(desktop)]
            if let Some(window) = _app.get_webview_window("main") {
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

    /// netkeiba の結果ページから、当日の馬場の偏り用に馬場と最後のコーナー順位が取れる（2023 天皇賞秋の前の 11R）
    #[tokio::test]
    async fn test_live_result_for_bias() {
        let client = scraper::build_client().unwrap();
        let (surface, rows) = scraper::fetch_result_for_bias(&client, "202305050811").await.expect("結果が取れる");
        assert_eq!(surface, "芝");
        assert!(rows.len() >= 10);
        let winner = rows.iter().find(|r| r.rank == 1).unwrap();
        assert_eq!((winner.umaban, winner.corner_last), (7, Some(6)), "1 着 7 番、通過 6-7-6");
        // 同じ日の先のレース
        assert_eq!(jvapp::earlier_race_ids("202305050803"), vec!["202305050801".to_string(), "202305050802".to_string()]);
    }

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

