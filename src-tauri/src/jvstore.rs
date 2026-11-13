//! JRA-VAN データの SQLite ストア。
//!
//! - 元データは JvDump の出力（追記だけのテキスト）。ここはそれを解析用に整理した写し。
//! - 取り込みは何度やり直しても同じ結果になる（キーで1件にまとめ、より確定に近いデータ区分だけが上書きする）。
//! - ファイルごとに前回どこまで読んだか（バイト位置）を覚え、追記された分だけを読む。
//! - データ区分 0（該当レコード削除）は、DB からも消す。
use crate::jvdata::{field, kubun_rank, parse_ra, parse_se, parse_work, race_key, JvRace, JvRunner, JvWork};
use rusqlite::{params, Connection, OptionalExtension};
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::Path;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS races (
    race_key TEXT PRIMARY KEY,
    race_id TEXT NOT NULL,
    date TEXT NOT NULL,
    kubun TEXT NOT NULL,
    kubun_rank INTEGER NOT NULL,
    venue_code TEXT, race_name TEXT, grade TEXT, race_type TEXT, condition_min TEXT,
    distance INTEGER, track_code INTEGER, surface TEXT, start_time TEXT, runners_count INTEGER,
    weather TEXT, turf_cond TEXT, dirt_cond TEXT,
    laps TEXT, first3f REAL, last3f REAL
);
CREATE INDEX IF NOT EXISTS races_date ON races(date);
CREATE TABLE IF NOT EXISTS runners (
    race_key TEXT NOT NULL,
    horse_id TEXT NOT NULL,
    kubun TEXT NOT NULL,
    kubun_rank INTEGER NOT NULL,
    waku INTEGER, umaban INTEGER, name TEXT, sex TEXT, age INTEGER, trainer TEXT, owner TEXT,
    carried_weight REAL, blinker INTEGER, jockey TEXT, jockey_name TEXT, apprentice TEXT,
    body_weight INTEGER, weight_diff INTEGER, abnormal TEXT, rank INTEGER, time_sec REAL,
    corner1 INTEGER, corner2 INTEGER, corner3 INTEGER, corner4 INTEGER,
    win_odds REAL, popularity INTEGER, last3f REAL, last4f REAL, time_diff REAL,
    mining_kubun TEXT, mining_time REAL, mining_rank INTEGER, style INTEGER,
    PRIMARY KEY (race_key, horse_id)
);
CREATE INDEX IF NOT EXISTS runners_horse ON runners(horse_id);
CREATE TABLE IF NOT EXISTS works (
    horse_id TEXT NOT NULL,
    slope INTEGER NOT NULL,
    center INTEGER NOT NULL,
    course INTEGER NOT NULL,
    date INTEGER NOT NULL,
    time INTEGER NOT NULL,
    t4 REAL, last1 REAL, lap2 REAL,
    PRIMARY KEY (horse_id, slope, center, date, time)
);
CREATE TABLE IF NOT EXISTS ingest_state (
    file TEXT PRIMARY KEY,
    offset INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);
";

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[derive(Debug, Default, serde::Serialize)]
pub struct IngestReport {
    pub file: String,
    pub from: u64,
    pub to: u64,
    pub upserted: u64,
    pub deleted: u64,
    pub skipped: u64,
}

/// out_dir（JvDump の出力先）の RACE/RA・RACE/SE・SLOP/HC・WOOD/WC を、前回の続きから取り込む
pub fn ingest_dir(conn: &mut Connection, out_dir: &Path) -> rusqlite::Result<Vec<IngestReport>> {
    ingest_dir_as(conn, out_dir, "")
}

/// ingest_dir と同じ。読んだ位置を `<prefix>RACE/RA.txt` のような名前で覚える
/// （年ごとに別に取り出したフォルダを取り込むとき、ほかのフォルダの位置と混ざらないように）
pub fn ingest_dir_as(conn: &mut Connection, out_dir: &Path, prefix: &str) -> rusqlite::Result<Vec<IngestReport>> {
    let mut reports = Vec::new();
    for rel in ["RACE/RA.txt", "RACE/SE.txt", "SLOP/HC.txt", "WOOD/WC.txt"] {
        let path = out_dir.join(rel);
        if path.exists() {
            reports.push(ingest_file(conn, &path, &format!("{}{}", prefix, rel))?);
        }
    }
    Ok(reports)
}

fn ingest_file(conn: &mut Connection, path: &Path, key: &str) -> rusqlite::Result<IngestReport> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let saved: u64 = conn
        .query_row("SELECT offset FROM ingest_state WHERE file = ?1", params![key], |r| r.get::<_, i64>(0))
        .optional()?
        .unwrap_or(0) as u64;
    // ファイルが前回より小さければ（取り直された）、最初から読む。キーでまとめるので重ねて読んでも同じ結果になる
    let from = if saved > size { 0 } else { saved };
    let mut rep = IngestReport { file: key.into(), from, to: from, ..Default::default() };
    let mut f = std::fs::File::open(path).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    f.seek(SeekFrom::Start(from)).ok();
    let mut reader = std::io::BufReader::with_capacity(1 << 20, f.by_ref());
    let tx = conn.transaction()?;
    {
        let mut buf = Vec::with_capacity(4096);
        let mut pos = from;
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf).unwrap_or(0);
            if n == 0 || buf.last() != Some(&b'\n') {
                break; // 書きかけの行は次回に回す
            }
            pos += n as u64;
            match apply_record(&tx, &buf)? {
                Applied::Upserted => rep.upserted += 1,
                Applied::Deleted => rep.deleted += 1,
                Applied::Skipped => rep.skipped += 1,
            }
        }
        rep.to = pos;
        tx.execute(
            "INSERT INTO ingest_state (file, offset, updated_at) VALUES (?1, ?2, datetime('now','localtime'))
             ON CONFLICT(file) DO UPDATE SET offset = excluded.offset, updated_at = excluded.updated_at",
            params![key, pos as i64],
        )?;
    }
    tx.commit()?;
    Ok(rep)
}

enum Applied {
    Upserted,
    Deleted,
    Skipped,
}

fn apply_record(tx: &rusqlite::Transaction, rec: &[u8]) -> rusqlite::Result<Applied> {
    if rec.len() < 3 {
        return Ok(Applied::Skipped);
    }
    let kind = &rec[0..2];
    let kubun = rec[2];
    // データ区分 0: 提供側で取り消されたレコード
    if kubun == b'0' {
        let n = match kind {
            b"RA" => tx.execute("DELETE FROM races WHERE race_key = ?1", params![race_key(rec)])?,
            b"SE" => tx.execute(
                "DELETE FROM runners WHERE race_key = ?1 AND horse_id = ?2",
                params![race_key(rec), field(rec, 31, 10)],
            )?,
            b"HC" | b"WC" => match parse_work_any(rec) {
                Some(w) => tx.execute(
                    "DELETE FROM works WHERE horse_id = ?1 AND slope = ?2 AND center = ?3 AND date = ?4 AND time = ?5",
                    params![w.horse_id, w.slope as i64, w.center as i64, w.date as i64, w.time as i64],
                )?,
                None => 0,
            },
            _ => 0,
        };
        return Ok(if n > 0 { Applied::Deleted } else { Applied::Skipped });
    }
    match kind {
        b"RA" => match parse_ra(rec) {
            Some(r) => {
                upsert_race(tx, &race_key(rec), &r)?;
                Ok(Applied::Upserted)
            }
            None => Ok(Applied::Skipped),
        },
        b"SE" => match parse_se(rec) {
            Some((key, h)) => {
                upsert_runner(tx, &key, &h)?;
                Ok(Applied::Upserted)
            }
            None => Ok(Applied::Skipped),
        },
        b"HC" | b"WC" => match parse_work(rec) {
            Some(w) => {
                upsert_work(tx, &w)?;
                Ok(Applied::Upserted)
            }
            None => Ok(Applied::Skipped),
        },
        _ => Ok(Applied::Skipped),
    }
}

/// 削除レコードは時刻などの値が欠けていることがあるので、キーに使う部分だけを読む
fn parse_work_any(rec: &[u8]) -> Option<JvWork> {
    let mut r = rec.to_vec();
    if r.len() > 2 {
        r[2] = b'1';
    }
    parse_work(&r)
}

fn upsert_race(tx: &rusqlite::Transaction, key: &str, r: &JvRace) -> rusqlite::Result<()> {
    let rank = kubun_rank(r.kubun as u8).unwrap_or(0) as i64;
    tx.prepare_cached(
        "INSERT INTO races (race_key, race_id, date, kubun, kubun_rank, venue_code, race_name, grade, race_type, condition_min,
            distance, track_code, surface, start_time, runners_count, weather, turf_cond, dirt_cond, laps, first3f, last3f)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT(race_key) DO UPDATE SET
            race_id = excluded.race_id, date = excluded.date, kubun = excluded.kubun, kubun_rank = excluded.kubun_rank,
            venue_code = excluded.venue_code, race_name = excluded.race_name, grade = excluded.grade,
            race_type = excluded.race_type, condition_min = excluded.condition_min, distance = excluded.distance,
            track_code = excluded.track_code, surface = excluded.surface, start_time = excluded.start_time,
            runners_count = excluded.runners_count, weather = excluded.weather, turf_cond = excluded.turf_cond,
            dirt_cond = excluded.dirt_cond, laps = excluded.laps, first3f = excluded.first3f, last3f = excluded.last3f
         WHERE excluded.kubun_rank >= races.kubun_rank",
    )?
    .execute(params![
        key,
        r.race_id,
        r.date,
        r.kubun.to_string(),
        rank,
        r.venue_code,
        r.race_name,
        r.grade,
        r.race_type,
        r.condition_min,
        r.distance as i64,
        r.track_code,
        r.surface,
        r.start_time,
        r.runners_count as i64,
        r.weather,
        r.turf_cond,
        r.dirt_cond,
        serde_json::to_string(&r.laps).unwrap_or_default(),
        r.first3f,
        r.last3f,
    ])?;
    Ok(())
}

fn upsert_runner(tx: &rusqlite::Transaction, key: &str, h: &JvRunner) -> rusqlite::Result<()> {
    let rank = kubun_rank(h.kubun as u8).unwrap_or(0) as i64;
    tx.prepare_cached(
        "INSERT INTO runners (race_key, horse_id, kubun, kubun_rank, waku, umaban, name, sex, age, trainer, owner,
            carried_weight, blinker, jockey, jockey_name, apprentice, body_weight, weight_diff, abnormal, rank, time_sec,
            corner1, corner2, corner3, corner4, win_odds, popularity, last3f, last4f, time_diff,
            mining_kubun, mining_time, mining_rank, style)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
            ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)
         ON CONFLICT(race_key, horse_id) DO UPDATE SET
            kubun = excluded.kubun, kubun_rank = excluded.kubun_rank, waku = excluded.waku, umaban = excluded.umaban,
            name = excluded.name, sex = excluded.sex, age = excluded.age, trainer = excluded.trainer, owner = excluded.owner,
            carried_weight = excluded.carried_weight, blinker = excluded.blinker, jockey = excluded.jockey,
            jockey_name = excluded.jockey_name, apprentice = excluded.apprentice, body_weight = excluded.body_weight,
            weight_diff = excluded.weight_diff, abnormal = excluded.abnormal, rank = excluded.rank,
            time_sec = excluded.time_sec, corner1 = excluded.corner1, corner2 = excluded.corner2,
            corner3 = excluded.corner3, corner4 = excluded.corner4, win_odds = excluded.win_odds,
            popularity = excluded.popularity, last3f = excluded.last3f, last4f = excluded.last4f,
            time_diff = excluded.time_diff, mining_kubun = excluded.mining_kubun, mining_time = excluded.mining_time,
            mining_rank = excluded.mining_rank, style = excluded.style
         WHERE excluded.kubun_rank >= runners.kubun_rank",
    )?
    .execute(params![
        key,
        h.horse_id,
        h.kubun.to_string(),
        rank,
        h.waku as i64,
        h.umaban as i64,
        h.name,
        h.sex,
        h.age as i64,
        h.trainer,
        h.owner,
        h.carried_weight,
        h.blinker as i64,
        h.jockey,
        h.jockey_name,
        h.apprentice,
        h.body_weight.map(|v| v as i64),
        h.weight_diff.map(|v| v as i64),
        h.abnormal,
        h.rank.map(|v| v as i64),
        h.time_sec,
        h.corners[0].map(|v| v as i64),
        h.corners[1].map(|v| v as i64),
        h.corners[2].map(|v| v as i64),
        h.corners[3].map(|v| v as i64),
        h.win_odds,
        h.popularity.map(|v| v as i64),
        h.last3f,
        h.last4f,
        h.time_diff,
        h.mining_kubun,
        h.mining_time,
        h.mining_rank.map(|v| v as i64),
        h.style as i64,
    ])?;
    Ok(())
}

fn upsert_work(tx: &rusqlite::Transaction, w: &JvWork) -> rusqlite::Result<()> {
    tx.prepare_cached(
        "INSERT INTO works (horse_id, slope, center, course, date, time, t4, last1, lap2)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(horse_id, slope, center, date, time) DO UPDATE SET
            course = excluded.course, t4 = excluded.t4, last1 = excluded.last1, lap2 = excluded.lap2",
    )?
    .execute(params![
        w.horse_id,
        w.slope as i64,
        w.center as i64,
        w.course as i64,
        w.date as i64,
        w.time as i64,
        w.t4,
        w.last1,
        w.lap2
    ])?;
    Ok(())
}

/// 期間 [from, to) の確定成績（データ区分 7）のレースを、出走馬つきで日付順に読む
pub fn load_races(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<Vec<JvRace>> {
    let mut stmt = conn.prepare(
        "SELECT race_key, race_id, date, kubun, venue_code, race_name, grade, race_type, condition_min, distance,
                track_code, surface, start_time, runners_count, weather, turf_cond, dirt_cond, laps, first3f, last3f
         FROM races WHERE date >= ?1 AND date < ?2 ORDER BY date, race_id",
    )?;
    let mut races: Vec<(String, JvRace)> = stmt
        .query_map(params![from, to], |r| {
            let laps: String = r.get(17)?;
            Ok((
                r.get::<_, String>(0)?,
                JvRace {
                    race_id: r.get(1)?,
                    date: r.get(2)?,
                    kubun: r.get::<_, String>(3)?.chars().next().unwrap_or(' '),
                    venue_code: r.get(4)?,
                    race_name: r.get(5)?,
                    grade: r.get(6)?,
                    race_type: r.get(7)?,
                    condition_min: r.get(8)?,
                    distance: r.get::<_, i64>(9)? as u32,
                    track_code: r.get(10)?,
                    surface: r.get(11)?,
                    start_time: r.get(12)?,
                    runners_count: r.get::<_, i64>(13)? as u32,
                    weather: r.get(14)?,
                    turf_cond: r.get(15)?,
                    dirt_cond: r.get(16)?,
                    laps: serde_json::from_str(&laps).unwrap_or_default(),
                    first3f: r.get(18)?,
                    last3f: r.get(19)?,
                    runners: Vec::new(),
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let index: std::collections::HashMap<String, usize> =
        races.iter().enumerate().map(|(i, (k, _))| (k.clone(), i)).collect();
    let mut stmt = conn.prepare(
        "SELECT r.race_key, r.kubun, r.waku, r.umaban, r.horse_id, r.name, r.sex, r.age, r.trainer, r.owner, r.carried_weight, r.blinker, r.jockey, r.jockey_name, r.apprentice, r.body_weight, r.weight_diff, r.abnormal, r.rank, r.time_sec, r.corner1, r.corner2, r.corner3, r.corner4, r.win_odds, r.popularity, r.last3f, r.last4f, r.time_diff, r.mining_kubun, r.mining_time, r.mining_rank, r.style
         FROM runners r JOIN races x ON x.race_key = r.race_key
         WHERE x.date >= ?1 AND x.date < ?2
         ORDER BY r.race_key, r.umaban",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        let u = |i: usize| -> rusqlite::Result<Option<u32>> { Ok(r.get::<_, Option<i64>>(i)?.map(|v| v as u32)) };
        Ok((
            r.get::<_, String>(0)?,
            JvRunner {
                kubun: r.get::<_, String>(1)?.chars().next().unwrap_or(' '),
                waku: r.get::<_, i64>(2)? as u32,
                umaban: r.get::<_, i64>(3)? as u32,
                horse_id: r.get(4)?,
                name: r.get(5)?,
                sex: r.get(6)?,
                age: r.get::<_, i64>(7)? as u32,
                trainer: r.get(8)?,
                owner: r.get(9)?,
                carried_weight: r.get(10)?,
                blinker: r.get::<_, i64>(11)? != 0,
                jockey: r.get(12)?,
                jockey_name: r.get(13)?,
                apprentice: r.get(14)?,
                body_weight: u(15)?,
                weight_diff: r.get::<_, Option<i64>>(16)?.map(|v| v as i32),
                abnormal: r.get(17)?,
                rank: u(18)?,
                time_sec: r.get(19)?,
                corners: [u(20)?, u(21)?, u(22)?, u(23)?],
                win_odds: r.get(24)?,
                popularity: u(25)?,
                last3f: r.get(26)?,
                last4f: r.get(27)?,
                time_diff: r.get(28)?,
                mining_kubun: r.get(29)?,
                mining_time: r.get(30)?,
                mining_rank: u(31)?,
                style: r.get::<_, i64>(32)? as u32,
            },
        ))
    })?;
    for row in rows {
        let (key, h) = row?;
        if let Some(&i) = index.get(&key) {
            races[i].1.runners.push(h);
        }
    }
    Ok(races.into_iter().map(|(_, r)| r).collect())
}

/// 調教を馬ごと・日付順に読む
pub fn load_works(conn: &Connection) -> rusqlite::Result<std::collections::HashMap<String, Vec<JvWork>>> {
    let mut stmt = conn.prepare(
        "SELECT horse_id, slope, center, course, date, time, t4, last1, lap2 FROM works ORDER BY horse_id, date, time",
    )?;
    let mut m: std::collections::HashMap<String, Vec<JvWork>> = std::collections::HashMap::new();
    let rows = stmt.query_map([], |r| {
        Ok(JvWork {
            horse_id: r.get(0)?,
            slope: r.get::<_, i64>(1)? != 0,
            center: r.get::<_, i64>(2)? as u8,
            course: r.get::<_, i64>(3)? as u8,
            date: r.get::<_, i64>(4)? as u32,
            time: r.get::<_, i64>(5)? as u32,
            t4: r.get(6)?,
            last1: r.get(7)?,
            lap2: r.get(8)?,
        })
    })?;
    for w in rows {
        let w = w?;
        m.entry(w.horse_id.clone()).or_default().push(w);
    }
    Ok(m)
}

/// アプリ用: 調教の基準を作る 2021年分と、`recent_from`（yyyymmdd）以降だけを読む（全期間を持つと重いため）
pub fn load_works_for(conn: &Connection, recent_from: u32) -> rusqlite::Result<std::collections::HashMap<String, Vec<JvWork>>> {
    let mut stmt = conn.prepare(
        "SELECT horse_id, slope, center, course, date, time, t4, last1, lap2 FROM works
         WHERE (date >= 20210101 AND date < 20220101) OR date >= ?1 ORDER BY horse_id, date, time",
    )?;
    let mut m: std::collections::HashMap<String, Vec<JvWork>> = std::collections::HashMap::new();
    let rows = stmt.query_map(params![recent_from as i64], |r| {
        Ok(JvWork {
            horse_id: r.get(0)?,
            slope: r.get::<_, i64>(1)? != 0,
            center: r.get::<_, i64>(2)? as u8,
            course: r.get::<_, i64>(3)? as u8,
            date: r.get::<_, i64>(4)? as u32,
            time: r.get::<_, i64>(5)? as u32,
            t4: r.get(6)?,
            last1: r.get(7)?,
            lap2: r.get(8)?,
        })
    })?;
    for w in rows {
        let w = w?;
        m.entry(w.horse_id.clone()).or_default().push(w);
    }
    Ok(m)
}

/// netkeiba 形式の race_id（12桁）のレースを、データ区分を問わず読む（出走馬名表・出馬表の段階でもよい）
pub fn race_by_id(conn: &Connection, race_id: &str) -> rusqlite::Result<Option<JvRace>> {
    let date: Option<String> = conn
        .query_row("SELECT date FROM races WHERE race_id = ?1", params![race_id], |r| r.get(0))
        .optional()?;
    let Some(d) = date else { return Ok(None) };
    let next = (d.parse::<u32>().unwrap_or(0) + 1).to_string();
    Ok(load_races(conn, &d, &next)?.into_iter().find(|r| r.race_id == race_id))
}

/// 最新のレース日（確定成績）
pub fn last_result_date(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT max(date) FROM races WHERE kubun = '7'", [], |r| r.get(0))
}
