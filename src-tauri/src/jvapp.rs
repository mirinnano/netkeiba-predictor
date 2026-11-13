//! アプリから JRA-VAN モデルを使うためのつなぎ。
//!
//! - DB の場所: 環境変数 KEIBA_DB、なければアプリのデータフォルダの keiba.db。
//! - Engine（過去成績の集計）は作るのに数秒かかるので、対象日と DB の更新時刻が同じあいだは使い回す。
//! - 予測するレースは netkeiba の出馬表を基本にし、netkeiba にない情報（調教師コード・ブリンカー・クラス）は
//!   DB にある JRA-VAN の出馬表で補う。netkeiba が使えないときは DB の出馬表だけで組み立てる。
use crate::jvdata::JvRace;
use crate::jvmodel::{self, Card, CardRunner, Engine};
use crate::jvstore;
use crate::models::{HorseEntry, JvModelOut, RaceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

pub fn db_path(app_data: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KEIBA_DB") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let p = app_data.join("keiba.db");
    p.exists().then_some(p)
}

struct Cached {
    path: PathBuf,
    date: u32,
    mtime: Option<SystemTime>,
    engine: Engine,
}

static ENGINE: Mutex<Option<Cached>> = Mutex::new(None);

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// "2023年11月26日" や "20231126" から yyyymmdd を取り出す
pub fn parse_date(s: &str) -> Option<u32> {
    let n: Vec<u32> = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
    match n.as_slice() {
        [y, m, d, ..] if *y > 1900 => Some(y * 10000 + m * 100 + d),
        [v] if *v > 19000000 => Some(*v),
        _ => None,
    }
}

/// netkeiba のグレード表記やレース名からクラスを推定する（DB に出馬表がないとき用）
fn class_from_netkeiba(info: &RaceInfo) -> f64 {
    let g = info.grade.clone().unwrap_or_default();
    let name = &info.race_name;
    match g.as_str() {
        "GI" => 7.0,
        "GII" => 6.0,
        "GIII" => 5.0,
        "L" => 4.5,
        "OP" => 4.0,
        _ if name.contains("新馬") || name.contains("未勝利") => 0.0,
        _ if name.contains("1勝") => 1.0,
        _ if name.contains("2勝") => 2.0,
        _ if name.contains("3勝") => 3.0,
        _ => 1.0,
    }
}

fn age_of(sex_age: &str) -> u32 {
    sex_age.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

/// JRA-VAN モデルの勝率と要素別の寄与。平地以外・DB にない日付などで使えないときは None
pub fn jv_probs(db: &Path, info: &RaceInfo, entries: &[HorseEntry]) -> Result<Option<JvModelOut>, String> {
    if !(info.surface == "芝" || info.surface == "ダ") {
        return Ok(None); // 障害戦は学習していない
    }
    let conn = jvstore::open(db).map_err(|e| format!("DB を開けません: {}", e))?;
    let jv_card = jvstore::race_by_id(&conn, &info.race_id).map_err(|e| e.to_string())?;
    let date = parse_date(&info.date_str)
        .or_else(|| jv_card.as_ref().and_then(|r| r.date.parse().ok()))
        .ok_or("レースの日付が分かりません")?;

    let started: Vec<&HorseEntry> = entries.iter().filter(|e| !e.is_scratched).collect();
    if started.len() < 2 {
        return Ok(None);
    }
    let by_horse: HashMap<&str, &crate::jvdata::JvRunner> =
        jv_card.as_ref().map(|r| r.runners.iter().map(|h| (h.horse_id.as_str(), h)).collect()).unwrap_or_default();
    let card = Card {
        date,
        venue_code: info.race_id.get(4..6).unwrap_or("").to_string(),
        surface: info.surface.clone(),
        distance: info.distance,
        class: jv_card
            .as_ref()
            .map(|r| jvmodel::class_level(&r.condition_min, &r.grade))
            .unwrap_or_else(|| class_from_netkeiba(info)),
        runners: started
            .iter()
            .map(|e| {
                let jv = by_horse.get(e.horse_id.as_str());
                CardRunner {
                    horse_id: e.horse_id.clone(),
                    umaban: e.umaban,
                    jockey: e.jockey_id.clone(),
                    trainer: jv.map(|h| h.trainer.clone()).unwrap_or_default(),
                    carried_weight: Some(e.carried_weight),
                    body_weight: e.body_weight,
                    weight_diff: e.weight_diff,
                    age: age_of(&e.sex_age),
                    blinker: jv.map(|h| h.blinker).unwrap_or(false),
                }
            })
            .collect(),
    };

    let mut guard = ENGINE.lock().map_err(|_| "Engine のロックに失敗しました")?;
    let fresh = match guard.as_ref() {
        Some(c) => c.path == db && c.date == date && c.mtime == mtime(db),
        None => false,
    };
    if !fresh {
        let engine = jvmodel::build_engine(&conn, date).map_err(|e| e.to_string())?;
        *guard = Some(Cached { path: db.to_path_buf(), date, mtime: mtime(db), engine });
    }
    let engine = &guard.as_ref().unwrap().engine;
    let x = engine.features(&card);
    let odds: Vec<Option<f64>> = started.iter().map(|e| e.win_odds).collect();
    let (p, _with_market, contrib) = jvmodel::win_probs_detail(&x, &odds);
    Ok(Some(JvModelOut {
        probs: card.runners.iter().zip(&p).map(|(h, p)| (h.umaban, *p)).collect(),
        contrib: card.runners.iter().zip(&contrib).map(|(h, c)| (h.umaban, c.to_vec())).collect(),
    }))
}

/// netkeiba が使えないとき用: DB の JRA-VAN 出馬表から、アプリの入力を組み立てる（過去走の詳細は持たない）
pub fn race_from_db(db: &Path, race_id: &str) -> Option<(RaceInfo, Vec<HorseEntry>)> {
    let conn = jvstore::open(db).ok()?;
    let r: JvRace = jvstore::race_by_id(&conn, race_id).ok()??;
    let venue = crate::scraper::venue_code_to_name(&r.venue_code).to_string();
    let grade = match r.grade.as_str() {
        "A" => Some("GI".to_string()),
        "B" => Some("GII".to_string()),
        "C" => Some("GIII".to_string()),
        "L" => Some("L".to_string()),
        _ => None,
    };
    let info = RaceInfo {
        race_id: r.race_id.clone(),
        race_name: if r.race_name.is_empty() { format!("{}R", &r.race_id[10..12]) } else { r.race_name.clone() },
        race_num: format!("{}R", r.race_id[10..12].trim_start_matches('0')),
        date_str: format!("{}年{}月{}日", &r.date[0..4], r.date[4..6].trim_start_matches('0'), r.date[6..8].trim_start_matches('0')),
        venue,
        grade,
        start_time: if r.start_time.len() == 4 { format!("{}:{}", &r.start_time[0..2], &r.start_time[2..4]) } else { "--:--".into() },
        surface: r.surface.clone(),
        distance: r.distance,
        direction: String::new(),
        weather: String::new(),
        track_cond: String::new(),
        head_count: 0,
        is_finished: r.kubun == '7',
        // 出走馬名表（データ区分 1）の段階では馬番が決まっていないので、並び順の仮番号を振る
        source_url: if r.runners.iter().all(|h| h.umaban == 0) {
            "JRA-VAN 出走馬名表（馬番は仮）".to_string()
        } else {
            format!("JRA-VAN（データ区分 {}）", r.kubun)
        },
    };
    let entries = r
        .runners
        .iter()
        .enumerate()
        .map(|(i, h)| HorseEntry {
            waku: h.waku,
            umaban: if h.umaban > 0 { h.umaban } else { i as u32 + 1 },
            horse_id: h.horse_id.clone(),
            horse_name: h.name.clone(),
            sex_age: format!("{}{}", match h.sex.as_str() { "1" => "牡", "2" => "牝", _ => "セ" }, h.age),
            carried_weight: h.carried_weight.unwrap_or(0.0),
            jockey_name: h.jockey_name.clone(),
            jockey_id: h.jockey.clone(),
            trainer_name: String::new(),
            sire: String::new(),
            dam: String::new(),
            broodmare_sire: String::new(),
            running_style: String::new(),
            interval_text: String::new(),
            body_weight: h.body_weight,
            weight_diff: h.weight_diff,
            win_odds: h.win_odds,
            place_odds_min: None,
            place_odds_max: None,
            popularity: h.popularity,
            is_scratched: !jvmodel::started(h),
            past_runs: Vec::new(),
            actual_rank: if r.kubun == '7' { h.rank } else { None },
            actual_time: None,
            actual_last_3f: if r.kubun == '7' { h.last3f } else { None },
        })
        .collect();
    Some((info, entries))
}

#[derive(Debug, Serialize)]
pub struct JvStatus {
    pub path: Option<String>,
    pub races: i64,
    pub runners: i64,
    pub last_result_date: Option<String>,
    pub latest_card_date: Option<String>,
}

pub fn status(db: Option<&Path>) -> JvStatus {
    let Some(p) = db else {
        return JvStatus { path: None, races: 0, runners: 0, last_result_date: None, latest_card_date: None };
    };
    let conn = match jvstore::open(p) {
        Ok(c) => c,
        Err(_) => return JvStatus { path: Some(p.display().to_string()), races: 0, runners: 0, last_result_date: None, latest_card_date: None },
    };
    let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    JvStatus {
        path: Some(p.display().to_string()),
        races: one("SELECT count(*) FROM races"),
        runners: one("SELECT count(*) FROM runners"),
        last_result_date: jvstore::last_result_date(&conn).ok().flatten(),
        latest_card_date: conn.query_row("SELECT max(date) FROM races", [], |r| r.get(0)).ok(),
    }
}
