//! JRA-VAN の JV-Data（JvDump が書き出した Shift_JIS の固定長レコード）を読む。
//!
//! 位置は JV-Data 仕様書 4.9.0.1 の「位置」（1 始まりのバイト位置）をそのまま書いている。
//! 同じレースが段階ごと（出馬表・速報成績・確定成績…）に何度も配信されるので、
//! レースごと・馬ごとに、もっとも確定に近いデータ区分の1件だけを残す。
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufRead;

/// 1 始まりの位置 `pos` から `len` バイトを取り出して Shift_JIS として読む（前後の空白は除く）
pub(crate) fn field(rec: &[u8], pos: usize, len: usize) -> String {
    let s = pos - 1;
    let e = (s + len).min(rec.len());
    if s >= e {
        return String::new();
    }
    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(&rec[s..e]);
    cow.trim_matches(|c: char| c == ' ' || c == '\u{3000}').to_string()
}

fn num(rec: &[u8], pos: usize, len: usize) -> Option<i64> {
    field(rec, pos, len).trim().parse().ok()
}

/// 数字だけの値を 1/10 にする（"0037" → 3.7、"336" → 33.6）。0 は「なし」とみなす
fn tenth(rec: &[u8], pos: usize, len: usize) -> Option<f64> {
    num(rec, pos, len).filter(|v| *v > 0).map(|v| v as f64 / 10.0)
}

/// データ区分の確定度。大きいほど確定に近い。0（削除）・9（中止）・A（地方）・B（海外）は使わない
pub(crate) fn kubun_rank(k: u8) -> Option<u8> {
    match k {
        b'7' => Some(7),
        b'6' => Some(6),
        b'5' => Some(5),
        b'4' => Some(4),
        b'3' => Some(3),
        b'2' => Some(2),
        b'1' => Some(1),
        _ => None,
    }
}

/// <コード表 2009.トラックコード>: 10〜22 芝、23〜29 ダート、51〜59 障害
pub fn surface_of_track(code: i64) -> &'static str {
    match code {
        10..=22 => "芝",
        23..=29 => "ダ",
        51..=59 => "障",
        _ => "",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvRace {
    /// netkeiba と同じ 12 桁（年 4・場 2・回 2・日 2・R 2）
    pub race_id: String,
    pub date: String, // "20231126"
    pub kubun: char,
    pub venue_code: String,
    pub race_name: String,
    pub grade: String,
    pub race_type: String,     // 競走種別コード
    pub condition_min: String, // 競走条件コード 最若年条件
    pub distance: u32,
    pub track_code: i64,
    pub surface: String,
    pub start_time: String,
    pub runners_count: u32,
    pub weather: String,
    pub turf_cond: String,
    pub dirt_cond: String,
    pub laps: Vec<f64>,
    pub first3f: Option<f64>,
    pub last3f: Option<f64>,
    pub runners: Vec<JvRunner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvRunner {
    pub kubun: char,
    pub waku: u32,
    pub umaban: u32,
    pub horse_id: String, // 血統登録番号
    pub name: String,
    pub sex: String,
    pub age: u32,
    pub trainer: String,
    pub owner: String,
    pub carried_weight: Option<f64>,
    pub blinker: bool,
    pub jockey: String,
    pub jockey_name: String,
    pub apprentice: String,
    pub body_weight: Option<u32>,
    pub weight_diff: Option<i32>,
    /// <コード表 2101.異常区分コード>（0 なら異常なし）
    pub abnormal: String,
    pub rank: Option<u32>,
    pub time_sec: Option<f64>,
    pub corners: [Option<u32>; 4],
    pub win_odds: Option<f64>,
    pub popularity: Option<u32>,
    pub last3f: Option<f64>,
    pub last4f: Option<f64>,
    /// 1着馬とのタイム差（秒）。1着馬は 2着馬との差（負）
    pub time_diff: Option<f64>,
    /// JRA-VAN のマイニング予想（1:前日 2:当日 3:直前）
    pub mining_kubun: String,
    pub mining_time: Option<f64>,
    pub mining_rank: Option<u32>,
    /// 今回レース脚質判定 1:逃 2:先 3:差 4:追
    pub style: u32,
}

pub(crate) fn race_key(rec: &[u8]) -> String {
    // 開催年(12,4) 月日(16,4) 場(20,2) 回(22,2) 日(24,2) R(26,2)
    field(rec, 12, 16)
}

fn netkeiba_id(rec: &[u8]) -> String {
    format!("{}{}{}{}{}", field(rec, 12, 4), field(rec, 20, 2), field(rec, 22, 2), field(rec, 24, 2), field(rec, 26, 2))
}

/// 走破タイム "MSSs"（9分99秒9）→ 秒
fn race_time(rec: &[u8], pos: usize) -> Option<f64> {
    let s = field(rec, pos, 4);
    if s.len() != 4 || s == "0000" {
        return None;
    }
    let m: f64 = s[0..1].parse().ok()?;
    let sec: f64 = s[1..3].parse().ok()?;
    let t: f64 = s[3..4].parse().ok()?;
    Some(m * 60.0 + sec + t / 10.0)
}

pub fn parse_ra(rec: &[u8]) -> Option<JvRace> {
    if rec.len() < 1270 || &rec[0..2] != b"RA" {
        return None;
    }
    let kubun = rec[2];
    kubun_rank(kubun)?;
    let track_code = num(rec, 706, 2).unwrap_or(0);
    let laps: Vec<f64> = (0..25).filter_map(|i| tenth(rec, 891 + i * 3, 3)).collect();
    Some(JvRace {
        race_id: netkeiba_id(rec),
        date: format!("{}{}", field(rec, 12, 4), field(rec, 16, 4)),
        kubun: kubun as char,
        venue_code: field(rec, 20, 2),
        race_name: field(rec, 33, 60),
        grade: field(rec, 615, 1),
        race_type: field(rec, 617, 2),
        condition_min: field(rec, 635, 3),
        distance: num(rec, 698, 4).unwrap_or(0) as u32,
        track_code,
        surface: surface_of_track(track_code).to_string(),
        start_time: field(rec, 874, 4),
        runners_count: num(rec, 884, 2).unwrap_or(0) as u32,
        weather: field(rec, 888, 1),
        turf_cond: field(rec, 889, 1),
        dirt_cond: field(rec, 890, 1),
        laps,
        first3f: tenth(rec, 970, 3),
        last3f: tenth(rec, 976, 3),
        runners: Vec::new(),
    })
}

pub fn parse_se(rec: &[u8]) -> Option<(String, JvRunner)> {
    if rec.len() < 553 || &rec[0..2] != b"SE" {
        return None;
    }
    let kubun = rec[2];
    kubun_rank(kubun)?;
    let sign = field(rec, 328, 1);
    let diff = num(rec, 329, 3).map(|d| if sign == "-" { -(d as i32) } else { d as i32 });
    let td = field(rec, 532, 4);
    let time_diff = if td.len() == 4 {
        let v: Option<f64> = td[1..].parse::<f64>().ok().map(|x| x / 10.0);
        match &td[0..1] {
            "-" => v.map(|x| -x),
            _ => v,
        }
    } else {
        None
    };
    let corner = |p: usize| num(rec, p, 2).filter(|v| *v > 0).map(|v| v as u32);
    Some((
        race_key(rec),
        JvRunner {
            kubun: kubun as char,
            waku: num(rec, 28, 1).unwrap_or(0) as u32,
            umaban: num(rec, 29, 2).unwrap_or(0) as u32,
            horse_id: field(rec, 31, 10),
            name: field(rec, 41, 36),
            sex: field(rec, 79, 1),
            age: num(rec, 83, 2).unwrap_or(0) as u32,
            trainer: field(rec, 86, 5),
            owner: field(rec, 99, 6),
            carried_weight: tenth(rec, 289, 3),
            blinker: field(rec, 295, 1) == "1",
            jockey: field(rec, 297, 5),
            jockey_name: field(rec, 307, 8),
            apprentice: field(rec, 323, 1),
            body_weight: num(rec, 325, 3).filter(|v| *v > 1 && *v < 999).map(|v| v as u32),
            weight_diff: diff,
            abnormal: field(rec, 332, 1),
            rank: num(rec, 335, 2).filter(|v| *v > 0).map(|v| v as u32),
            time_sec: race_time(rec, 339),
            corners: [corner(352), corner(354), corner(356), corner(358)],
            win_odds: tenth(rec, 360, 4),
            popularity: num(rec, 364, 2).filter(|v| *v > 0).map(|v| v as u32),
            last4f: tenth(rec, 388, 3),
            last3f: tenth(rec, 391, 3),
            time_diff,
            mining_kubun: field(rec, 537, 1),
            mining_time: num(rec, 538, 5).filter(|v| *v > 0).map(|v| {
                // 9分99秒99
                let s = format!("{:05}", v);
                let m: f64 = s[0..1].parse().unwrap_or(0.0);
                let sec: f64 = s[1..3].parse().unwrap_or(0.0);
                let cs: f64 = s[3..5].parse().unwrap_or(0.0);
                m * 60.0 + sec + cs / 100.0
            }),
            mining_rank: num(rec, 551, 2).filter(|v| *v > 0).map(|v| v as u32),
            style: num(rec, 553, 1).unwrap_or(0) as u32,
        },
    ))
}

fn for_each_record(path: &std::path::Path, mut f: impl FnMut(&[u8])) -> std::io::Result<()> {
    let file = std::fs::File::open(path)?;
    let mut r = std::io::BufReader::with_capacity(1 << 20, file);
    let mut buf = Vec::with_capacity(4096);
    loop {
        buf.clear();
        if r.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        f(&buf);
    }
    Ok(())
}

/// RACE/RA.txt と RACE/SE.txt から、レースごと・馬ごとにもっとも確定に近い1件を残して組み立てる
pub fn load_races(race_dir: &std::path::Path) -> std::io::Result<Vec<JvRace>> {
    let mut races: HashMap<String, JvRace> = HashMap::new();
    for_each_record(&race_dir.join("RA.txt"), |rec| {
        if let Some(r) = parse_ra(rec) {
            let key = race_key(rec);
            let better = match races.get(&key) {
                Some(old) => kubun_rank(r.kubun as u8) >= kubun_rank(old.kubun as u8),
                None => true,
            };
            if better {
                races.insert(key, r);
            }
        }
    })?;
    let mut runners: HashMap<(String, String), JvRunner> = HashMap::new();
    for_each_record(&race_dir.join("SE.txt"), |rec| {
        if let Some((key, h)) = parse_se(rec) {
            let k = (key, h.horse_id.clone());
            let better = match runners.get(&k) {
                Some(old) => kubun_rank(h.kubun as u8) >= kubun_rank(old.kubun as u8),
                None => true,
            };
            if better {
                runners.insert(k, h);
            }
        }
    })?;
    for ((key, _), h) in runners {
        if let Some(r) = races.get_mut(&key) {
            r.runners.push(h);
        }
    }
    let mut out: Vec<JvRace> = races.into_values().collect();
    for r in &mut out {
        r.runners.sort_by_key(|h| h.umaban);
    }
    out.sort_by(|a, b| (&a.date, &a.race_id).cmp(&(&b.date, &b.race_id)));
    Ok(out)
}


/// 調教1本（坂路 HC・ウッドチップ WC）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvWork {
    pub horse_id: String,
    pub slope: bool,
    /// 0:美浦 1:栗東
    pub center: u8,
    /// ウッドのコース（0:A〜4:E）。坂路は 0
    pub course: u8,
    pub date: u32,
    pub time: u32,
    pub t4: Option<f64>,
    pub last1: Option<f64>,
    /// ラスト1F の1つ前のラップ（400M〜200M）
    pub lap2: Option<f64>,
}

/// 測定不良（0）や上限（999.9・99.9）は None
fn work_time(rec: &[u8], pos: usize, len: usize) -> Option<f64> {
    let s = field(rec, pos, len);
    if s.chars().all(|c| c == '9') {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| *v > 0.0).map(|v| v / 10.0)
}

/// HC（坂路）: 4F合計 35(4) / 400-200M 53(3) / 200-0M 56(3)
/// WC（ウッド）: コース 35(1) / 4F合計 80(4) / 400-200M 98(3) / 200-0M 101(3)
pub fn parse_work(rec: &[u8]) -> Option<JvWork> {
    let slope = match rec.get(0..2) {
        Some(b"HC") => true,
        Some(b"WC") => false,
        _ => return None,
    };
    if rec.len() < if slope { 58 } else { 103 } {
        return None;
    }
    let date: u32 = field(rec, 13, 8).parse().ok()?;
    Some(JvWork {
        horse_id: field(rec, 25, 10),
        slope,
        center: rec[11].wrapping_sub(b'0'),
        course: if slope { 0 } else { rec[34].wrapping_sub(b'0') },
        date,
        time: field(rec, 21, 4).parse().unwrap_or(0),
        t4: if slope { work_time(rec, 35, 4) } else { work_time(rec, 80, 4) },
        last1: if slope { work_time(rec, 56, 3) } else { work_time(rec, 101, 3) },
        lap2: if slope { work_time(rec, 53, 3) } else { work_time(rec, 98, 3) },
    })
}
