//! 保存した予想の払戻を、アプリが後ろに回っていても見張る。
//! 発走 60 分前から 45 分後までのレースを 20 秒ごとに見て、
//! 発走 10 分前に一度知らせ、発走 1 分後からは netkeiba の結果ページで払戻を待つ。
//! 払戻が出たら照合して保存し、通知する。画面が前に戻ったときに演出を出せるよう、結果を溜めておく。

use crate::{db, models::RacePredictionResult, predictor, scraper};
use chrono::{Local, NaiveDate, NaiveTime, TimeZone};
use std::{collections::HashSet, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// 画面がまだ受け取っていない確定結果
#[derive(Default)]
pub struct Pending(pub Mutex<Vec<RacePredictionResult>>);

fn post_time(r: &RacePredictionResult) -> Option<chrono::DateTime<Local>> {
    let nums: Vec<u32> = r
        .race_info
        .date_str
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let d = NaiveDate::from_ymd_opt(*nums.first()? as i32, *nums.get(1)?, *nums.get(2)?)?;
    let t = NaiveTime::parse_from_str(r.race_info.start_time.trim(), "%H:%M").ok()?;
    Local.from_local_datetime(&d.and_time(t)).single()
}

/// 自分の馬券（なければ買い目）の購入額・払戻・当たった馬券
pub fn my_totals(r: &RacePredictionResult) -> (u32, u32, Vec<String>) {
    let bets: Vec<_> = if r.my_bets.is_empty() {
        r.recommendations.iter().filter(|b| b.suggested_amount > 0).collect()
    } else {
        r.my_bets.iter().collect()
    };
    let bet = bets.iter().map(|b| b.suggested_amount).sum();
    let ret = bets.iter().map(|b| b.actual_return.unwrap_or(0)).sum();
    let hits = bets
        .iter()
        .filter(|b| b.is_hit == Some(true))
        .map(|b| format!("{} {}", b.bet_type, b.horses.iter().map(|h| h.to_string()).collect::<Vec<_>>().join("-")))
        .collect();
    (bet, ret, hits)
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

pub fn spawn(app: AppHandle, db_path: PathBuf, app_data: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let mut reminded: HashSet<String> = HashSet::new();
        let flag = app_data.join("watching");
        loop {
            let now = Local::now();
            let mut active = 0;
            for r in db::unverified_predictions(&db_path).unwrap_or_default() {
                let Some(post) = post_time(&r) else { continue };
                let mins = (now - post).num_seconds() as f64 / 60.0;
                if !(-60.0..=45.0).contains(&mins) {
                    continue;
                }
                active += 1;
                let id = r.race_info.race_id.clone();
                let name = r.race_info.race_name.clone();
                if (-10.0..-8.0).contains(&mins) && reminded.insert(id.clone()) {
                    notify(&app, &format!("あと10分で発走：{}", name), &format!("{}発走。オッズとおすすめを確かめる", r.race_info.start_time));
                }
                if mins < 1.0 {
                    continue;
                }
                let Ok((rows, payouts)) = scraper::fetch_race_result(&id).await else { continue };
                if rows.is_empty() || payouts.is_empty() {
                    continue;
                }
                let done = predictor::apply_official_result(r, &rows, payouts);
                let Ok(saved) = db::save_prediction(&db_path, done) else { continue };
                let (bet, ret, hits) = my_totals(&saved);
                if ret > 0 {
                    notify(
                        &app,
                        &format!("的中 {}円：{}", ret.to_string_thousands(), name),
                        &format!("{}（{}円 → {}円）", hits.join("・"), bet.to_string_thousands(), ret.to_string_thousands()),
                    );
                } else {
                    notify(&app, &format!("確定：{}", name), "今回は外れです");
                }
                if let Some(p) = app.try_state::<Pending>() {
                    p.0.lock().unwrap().push(saved);
                }
                let _ = app.emit("payout", id);
            }
            // Android の前面サービスは、この印があるあいだだけ動く
            if active > 0 {
                let _ = std::fs::write(&flag, active.to_string());
            } else {
                let _ = std::fs::remove_file(&flag);
            }
            tokio::time::sleep(Duration::from_secs(20)).await;
        }
    });
}

trait Thousands {
    fn to_string_thousands(&self) -> String;
}
impl Thousands for u32 {
    fn to_string_thousands(&self) -> String {
        let s = self.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    }
}
