use crate::models::{RacePredictionResult, RoiDashboardStats, SavedPredictionSummary};
use chrono::Local;
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub fn get_db_connection(db_path: &PathBuf) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn =
        Connection::open(db_path).map_err(|e| format!("SQLiteデータベース接続エラー: {}", e))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS predictions (
            race_id TEXT PRIMARY KEY,
            race_date TEXT NOT NULL,
            venue TEXT NOT NULL,
            race_num TEXT NOT NULL,
            race_name TEXT NOT NULL,
            grade TEXT,
            surface TEXT NOT NULL,
            distance INTEGER NOT NULL,
            honmei_name TEXT NOT NULL,
            honmei_umaban INTEGER NOT NULL,
            honmei_actual_rank INTEGER,
            total_bet INTEGER NOT NULL,
            total_return INTEGER,
            is_verified INTEGER NOT NULL DEFAULT 0,
            memo TEXT NOT NULL DEFAULT '',
            saved_at TEXT NOT NULL,
            full_json TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("テーブル作成エラー: {}", e))?;

    Ok(conn)
}

pub fn save_prediction(
    db_path: &PathBuf,
    mut result: RacePredictionResult,
) -> Result<RacePredictionResult, String> {
    let conn = get_db_connection(db_path)?;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    result.saved_at = Some(now.clone());

    let honmei = result
        .predictions
        .iter()
        .find(|p| p.mark == "◎")
        .or_else(|| result.predictions.first());

    let honmei_name = honmei
        .map(|p| p.entry.horse_name.clone())
        .unwrap_or_else(|| "-".to_string());
    let honmei_umaban = honmei.map(|p| p.entry.umaban).unwrap_or(1);
    let honmei_actual_rank = honmei.and_then(|p| p.entry.actual_rank);

    let is_verified = if result.race_info.is_finished { 1 } else { 0 };

    let full_json =
        serde_json::to_string(&result).map_err(|e| format!("JSONシリアライズエラー: {}", e))?;

    conn.execute(
        "INSERT INTO predictions (
            race_id, race_date, venue, race_num, race_name, grade, surface, distance,
            honmei_name, honmei_umaban, honmei_actual_rank, total_bet, total_return,
            is_verified, memo, saved_at, full_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(race_id) DO UPDATE SET
            race_date = excluded.race_date,
            venue = excluded.venue,
            race_num = excluded.race_num,
            race_name = excluded.race_name,
            grade = excluded.grade,
            surface = excluded.surface,
            distance = excluded.distance,
            honmei_name = excluded.honmei_name,
            honmei_umaban = excluded.honmei_umaban,
            honmei_actual_rank = excluded.honmei_actual_rank,
            total_bet = excluded.total_bet,
            total_return = excluded.total_return,
            is_verified = excluded.is_verified,
            memo = excluded.memo,
            saved_at = excluded.saved_at,
            full_json = excluded.full_json",
        params![
            result.race_info.race_id,
            result.race_info.date_str,
            result.race_info.venue,
            result.race_info.race_num,
            result.race_info.race_name,
            result.race_info.grade,
            result.race_info.surface,
            result.race_info.distance,
            honmei_name,
            honmei_umaban,
            honmei_actual_rank,
            result.total_bet,
            result.total_return,
            is_verified,
            result.memo,
            now,
            full_json
        ],
    )
    .map_err(|e| format!("データベース保存エラー: {}", e))?;

    Ok(result)
}

pub fn load_prediction(db_path: &PathBuf, race_id: &str) -> Result<RacePredictionResult, String> {
    let conn = get_db_connection(db_path)?;
    let full_json: String = conn
        .query_row(
            "SELECT full_json FROM predictions WHERE race_id = ?1",
            params![race_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("保存済みデータの取得エラー: {}", e))?;

    serde_json::from_str(&full_json).map_err(|e| format!("JSONデシリアライズエラー: {}", e))
}

pub fn delete_prediction(db_path: &PathBuf, race_id: &str) -> Result<(), String> {
    let conn = get_db_connection(db_path)?;
    conn.execute(
        "DELETE FROM predictions WHERE race_id = ?1",
        params![race_id],
    )
    .map_err(|e| format!("削除エラー: {}", e))?;
    Ok(())
}

pub fn get_dashboard_stats(db_path: &PathBuf) -> Result<RoiDashboardStats, String> {
    let conn = get_db_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT race_id, race_date, venue, race_num, race_name, grade, surface, distance,
                    honmei_name, honmei_umaban, honmei_actual_rank, total_bet, total_return,
                    is_verified, memo, saved_at
             FROM predictions
             ORDER BY saved_at DESC",
        )
        .map_err(|e| format!("クエリエラー: {}", e))?;

    let mut total_saved_races = 0_u32;
    let mut verified_races = 0_u32;
    let mut honmei_win_count = 0_u32;
    let mut honmei_top3_count = 0_u32;
    let mut ticket_hit_races = 0_u32;
    let mut total_investment = 0_u32;
    let mut total_return = 0_u32;
    let mut recent_history = Vec::new();

    let rows = stmt
        .query_map([], |row| {
            let is_ver_int: i32 = row.get(13)?;
            let honmei_rank: Option<u32> = row.get(10)?;
            let t_bet: u32 = row.get(11)?;
            let t_ret: Option<u32> = row.get(12)?;
            Ok((
                SavedPredictionSummary {
                    race_id: row.get(0)?,
                    race_date: row.get(1)?,
                    venue: row.get(2)?,
                    race_num: row.get(3)?,
                    race_name: row.get(4)?,
                    grade: row.get(5)?,
                    surface: row.get(6)?,
                    distance: row.get(7)?,
                    honmei_name: row.get(8)?,
                    honmei_umaban: row.get(9)?,
                    total_bet: t_bet,
                    total_return: t_ret,
                    is_verified: is_ver_int != 0,
                    memo: row.get(14)?,
                    saved_at: row.get(15)?,
                },
                honmei_rank,
            ))
        })
        .map_err(|e| format!("データ読込エラー: {}", e))?;

    for r in rows {
        let (summary, honmei_rank) = r.map_err(|e| e.to_string())?;
        total_saved_races += 1;
        if summary.is_verified {
            verified_races += 1;
            total_investment += summary.total_bet;
            let ret = summary.total_return.unwrap_or(0);
            total_return += ret;
            if ret > 0 {
                ticket_hit_races += 1;
            }
            if let Some(rank) = honmei_rank {
                if rank == 1 {
                    honmei_win_count += 1;
                }
                if rank <= 3 {
                    honmei_top3_count += 1;
                }
            }
        }
        recent_history.push(summary);
    }

    let net_profit = (total_return as i64) - (total_investment as i64);
    let overall_roi = if total_investment > 0 {
        ((total_return as f64 / total_investment as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    Ok(RoiDashboardStats {
        total_saved_races,
        verified_races,
        honmei_win_count,
        honmei_top3_count,
        ticket_hit_races,
        total_investment,
        total_return,
        net_profit,
        overall_roi,
        recent_history,
    })
}
