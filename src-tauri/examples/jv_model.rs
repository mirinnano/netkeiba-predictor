//! JRA-VAN の全レースから「そのレースより前に分かっていたこと」だけで特徴量を作り、
//! 市場（単勝オッズ）に足して良くなるかを測る。2026年は最終判定用なので読まない。
//!
//!   cargo run --release --example jv_model -- <jv_races.jsonl>
//!
//! 情報漏れを防ぐ決まり:
//! - 日付順に1日ずつ処理し、その日の全レースの特徴量を作ってから、その日の結果を履歴に足す。
//! - 基準タイムとペースの基準は 2021年のレースだけで作る（検証区間は 2023年以降）。
use netkeiba_predictor_lib::jvdata::{JvRace, JvRunner};
use std::collections::HashMap;
use std::io::BufRead;

const NAMES: [&str; 43] = [
    "指数最高3", "指数平均3", "指数前走", "上がり順位", "展開不利", "4角から挽回", "着差", "着順率", "過去人気",
    "人気以上", "クラス変化", "騎手3着内", "調教師3着内", "騎手勝率", "間隔", "距離変更", "芝ダ替わり", "斤量差",
    "馬体重増減", "馬体重", "馬齢", "出走数", "ブリンカー", "初ブリンカー", "馬番位置", "脚質", "同場3着内", "同距離3着内",
    "乗り替わり", "騎手の格上げ",
    "坂路4F", "坂路ラスト1F", "坂路本数", "ウッド4F", "ウッドラスト1F", "ウッド本数", "最終追切からの日数", "最終追切ラスト1F",
    "最終追切の加速", "自己比ラスト1F", "併せ馬", "28日本数", "最終が坂路",
];
const GROUPS: [(&str, &[usize]); 10] = [
    ("時計", &[0, 1, 2]),
    ("上がり・展開", &[3, 4, 5]),
    ("着順・着差", &[6, 7, 9]),
    ("過去人気", &[8]),
    ("クラス", &[10]),
    ("騎手・調教師", &[11, 12, 13, 28, 29]),
    ("状態・ローテ", &[14, 15, 16, 17, 18, 19, 21, 22, 23]),
    ("その他", &[20, 24, 25, 26, 27]),
    ("調教", &[30, 31, 32, 33, 34, 35, 36, 37]),
    ("調教(詳細)", &[38, 39, 40, 41, 42]),
];

#[derive(Clone)]
struct Run {
    date: u32,
    venue: String,
    surface: String,
    distance: u32,
    class: f64,
    rank: Option<u32>,
    n: u32,
    time_diff: Option<f64>,
    fig: Option<f64>,
    l3f_ratio: Option<f64>,
    pos4: Option<f64>,
    pace: Option<f64>,
    pop: Option<u32>,
    style: u32,
    blinker: bool,
    jockey: String,
}

#[derive(Default, Clone, Copy)]
struct Stat {
    n: f64,
    win: f64,
    top3: f64,
}

/// 調教1本。時間は秒、基準（2021年のトレセン・種類ごとの中央値）との差に直す前の値
#[derive(Clone)]
struct Work {
    date: u32,
    slope: bool,
    center: u8,
    course: u8,
    t4: Option<f64>,
    last1: Option<f64>,
    /// ラスト1F の1つ前のラップ（400M〜200M）
    lap2: Option<f64>,
    time: u32,
    /// 同じトレセン・同じ日・同じ時刻・同じ種類で調教した他の馬の数（併せ馬の目安）
    partners: u32,
}

fn tenth_field(rec: &[u8], pos: usize, len: usize) -> Option<f64> {
    let s = std::str::from_utf8(&rec[pos - 1..pos - 1 + len]).ok()?;
    let v: f64 = s.trim().parse().ok()?;
    if v <= 0.0 || s.chars().all(|c| c == '9') { None } else { Some(v / 10.0) }
}

fn int_field(rec: &[u8], pos: usize, len: usize) -> u32 {
    std::str::from_utf8(&rec[pos - 1..pos - 1 + len]).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// SLOP/HC.txt（坂路）と WOOD/WC.txt（ウッドチップ）を読み、馬ごとに日付順の調教リストにする
fn load_works(out_dir: &std::path::Path) -> HashMap<String, Vec<Work>> {
    let mut m: HashMap<String, Vec<Work>> = HashMap::new();
    for (file, slope, len) in [("SLOP/HC.txt", true, 58usize), ("WOOD/WC.txt", false, 103usize)] {
        let Ok(f) = std::fs::File::open(out_dir.join(file)) else {
            eprintln!("調教データがありません: {}", file);
            continue;
        };
        let mut r = std::io::BufReader::with_capacity(1 << 20, f);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            if r.read_until(b'\n', &mut buf).unwrap_or(0) == 0 {
                break;
            }
            if buf.len() < len || buf[2] == b'0' {
                continue; // データ区分 0 = 削除
            }
            let horse = String::from_utf8_lossy(&buf[24..34]).to_string();
            let w = if slope {
                // HC: 4F合計 35(4) / ラップ 200-0M 56(3)
                Work { date: int_field(&buf, 13, 8), slope, center: buf[11] - b'0', course: 0, t4: tenth_field(&buf, 35, 4), last1: tenth_field(&buf, 56, 3), lap2: tenth_field(&buf, 53, 3), time: int_field(&buf, 21, 4), partners: 0 }
            } else {
                // WC: コース 35(1) / 4F合計 80(4) / ラップ 200-0M 101(3)
                Work { date: int_field(&buf, 13, 8), slope, center: buf[11] - b'0', course: buf[34] - b'0', t4: tenth_field(&buf, 80, 4), last1: tenth_field(&buf, 101, 3), lap2: tenth_field(&buf, 98, 3), time: int_field(&buf, 21, 4), partners: 0 }
            };
            m.entry(horse).or_default().push(w);
        }
    }
    finish_works(&mut m);
    m
}

/// SQLite（jv_ingest で作った DB）から調教を読む
fn load_works_db(conn: &rusqlite::Connection) -> HashMap<String, Vec<Work>> {
    let mut m: HashMap<String, Vec<Work>> = HashMap::new();
    for (horse, v) in netkeiba_predictor_lib::jvstore::load_works(conn).expect("調教を読めません") {
        m.insert(
            horse,
            v.into_iter()
                .map(|w| Work { date: w.date, slope: w.slope, center: w.center, course: w.course, t4: w.t4, last1: w.last1, lap2: w.lap2, time: w.time, partners: 0 })
                .collect(),
        );
    }
    finish_works(&mut m);
    m
}

/// 併せ馬の目安を数え、日付・時刻順に並べる
fn finish_works(m: &mut HashMap<String, Vec<Work>>) {
    // 併せ馬の目安: 同じ (種類, トレセン, コース, 日, 時刻) に何頭いたか
    let mut slots: HashMap<(bool, u8, u8, u32, u32), u32> = HashMap::new();
    for v in m.values() {
        for w in v {
            *slots.entry((w.slope, w.center, w.course, w.date, w.time)).or_default() += 1;
        }
    }
    for v in m.values_mut() {
        for w in v.iter_mut() {
            w.partners = slots.get(&(w.slope, w.center, w.course, w.date, w.time)).copied().unwrap_or(1) - 1;
        }
        v.sort_by_key(|w| (w.date, w.time));
    }
}

fn work_key(w: &Work) -> (bool, u8, u8) {
    (w.slope, w.center, if w.slope { 0 } else { w.course })
}

struct Runner {
    market: f64,
    odds: f64,
    win: bool,
    x: Vec<f64>,
}

fn class_level(r: &JvRace) -> f64 {
    let base = match r.condition_min.as_str() {
        "701" | "702" | "703" => 0.0,
        "005" => 1.0,
        "010" => 2.0,
        "016" => 3.0,
        "999" => 4.0,
        _ => 1.0,
    };
    match r.grade.as_str() {
        "A" => 7.0,
        "B" => 6.0,
        "C" | "D" => 5.0,
        "L" => 4.5,
        _ => base,
    }
}

fn going(r: &JvRace) -> String {
    if r.surface == "芝" { r.turf_cond.clone() } else { r.dirt_cond.clone() }
}

fn ymd_days(d: u32) -> i64 {
    let y = (d / 10000) as i32;
    let m = (d / 100 % 100) as u32;
    let dd = (d % 100) as u32;
    chrono::NaiveDate::from_ymd_opt(y, m, dd).map(|x| x.num_days_from_ce() as i64).unwrap_or(0)
}
use chrono::Datelike;

fn median(v: &mut Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn started(h: &JvRunner) -> bool {
    !matches!(h.abnormal.as_str(), "1" | "2" | "3")
}

fn logit_rate(hits: f64, n: f64, prior: f64, k: f64) -> f64 {
    let p = (hits + prior * k) / (n + k);
    (p / (1.0 - p)).ln()
}

fn standardize(col: &mut [f64]) {
    let n = col.len() as f64;
    let m = col.iter().sum::<f64>() / n;
    let sd = (col.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n).sqrt();
    for x in col.iter_mut() {
        *x = if sd > 1e-9 { (*x - m) / sd } else { 0.0 };
    }
}

fn main() {
    // 引数が .db なら SQLite（jv_ingest で作ったもの）から、そうでなければ jv_races.jsonl と JvDump の出力から読む
    let path = std::env::args().nth(1).expect("keiba.db か jv_races.jsonl を指定してください");
    let db = if path.ends_with(".db") { Some(netkeiba_predictor_lib::jvstore::open(std::path::Path::new(&path)).expect("DB を開けません")) } else { None };
    let works = match &db {
        Some(c) => load_works_db(c),
        None => load_works(&std::path::PathBuf::from(std::env::args().nth(2).unwrap_or_else(|| "../backtest/jvdata/out".into()))),
    };
    // 調教タイムの基準: 2021年の、坂路/ウッド × トレセン × コースごとの中央値
    let mut wv: HashMap<(bool, u8, u8), (Vec<f64>, Vec<f64>)> = HashMap::new();
    for v in works.values() {
        for w in v.iter().filter(|w| w.date < 20220101) {
            let e = wv.entry(work_key(w)).or_default();
            if let Some(t) = w.t4 { e.0.push(t); }
            if let Some(t) = w.last1 { e.1.push(t); }
        }
    }
    let work_std: HashMap<(bool, u8, u8), (f64, f64)> = wv
        .into_iter()
        .filter(|(_, (a, b))| a.len() >= 100 && b.len() >= 100)
        .map(|(k, (mut a, mut b))| (k, (median(&mut a), median(&mut b))))
        .collect();
    println!("調教: {} 頭分 / 基準 {} 種類", works.len(), work_std.len());
    // 2026年は最終判定用なので読まない
    let mut races: Vec<JvRace> = match &db {
        Some(c) => netkeiba_predictor_lib::jvstore::load_races(c, "20210101", "20260101").expect("レースを読めません"),
        None => std::io::BufReader::new(std::fs::File::open(&path).unwrap())
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str::<JvRace>(&l).ok())
            .filter(|r| r.date.as_str() >= "20210101" && r.date.as_str() < "20260101")
            .collect(),
    };
    races.retain(|r| r.kubun == '7');
    races.sort_by(|a, b| (&a.date, &a.race_id).cmp(&(&b.date, &b.race_id)));

    // 基準タイム（会場・トラック・距離ごとの完走馬タイムの中央値）と馬場補正、ペースの基準は 2021年だけで作る
    let mut tv: HashMap<String, Vec<f64>> = HashMap::new();
    let mut pv: HashMap<String, Vec<f64>> = HashMap::new();
    for r in races.iter().filter(|r| r.date.as_str() < "20220101" && r.surface != "障") {
        let key = format!("{}|{}|{}", r.venue_code, r.track_code, r.distance);
        for h in &r.runners {
            if let (Some(t), "1") = (h.time_sec, going(r).as_str()) {
                tv.entry(key.clone()).or_default().push(t);
            }
        }
        if let (Some(f), Some(l)) = (r.first3f, r.last3f) {
            pv.entry(format!("{}|{}", r.surface, r.distance)).or_default().push(f - l);
        }
    }
    let std_t: HashMap<String, f64> = tv.into_iter().filter(|(_, v)| v.len() >= 30).map(|(k, mut v)| (k, median(&mut v))).collect();
    let mut gv: HashMap<String, Vec<f64>> = HashMap::new();
    for r in races.iter().filter(|r| r.date.as_str() < "20220101" && r.surface != "障") {
        let key = format!("{}|{}|{}", r.venue_code, r.track_code, r.distance);
        if let Some(b) = std_t.get(&key) {
            for h in &r.runners {
                if let Some(t) = h.time_sec {
                    gv.entry(format!("{}|{}", r.surface, going(r))).or_default().push((t - b) / r.distance as f64 * 1000.0);
                }
            }
        }
    }
    let going_adj: HashMap<String, f64> = gv.into_iter().map(|(k, mut v)| (k, median(&mut v))).collect();
    let pace_std: HashMap<String, f64> = pv.into_iter().filter(|(_, v)| v.len() >= 20).map(|(k, mut v)| (k, median(&mut v))).collect();
    println!("基準タイム {} 条件 / ペース基準 {} 条件（2021年から）", std_t.len(), pace_std.len());

    let mut hist: HashMap<String, Vec<Run>> = HashMap::new();
    let mut jockey: HashMap<String, Stat> = HashMap::new();
    let mut trainer: HashMap<String, Stat> = HashMap::new();
    let mut data: Vec<(String, Vec<Runner>)> = Vec::new();
    let mut coverage = (0usize, 0usize);

    let mut i = 0;
    while i < races.len() {
        let day = races[i].date.clone();
        let mut j = i;
        while j < races.len() && races[j].date == day {
            j += 1;
        }
        let today = &races[i..j];
        let dnum: u32 = day.parse().unwrap();
        // 1) その日の特徴量を、前日までの履歴だけで作る
        for r in today.iter().filter(|r| r.surface == "芝" || r.surface == "ダ") {
            let st: Vec<&JvRunner> = r.runners.iter().filter(|h| started(h)).collect();
            if st.len() < 5 || st.iter().any(|h| h.win_odds.is_none()) || !st.iter().any(|h| h.rank == Some(1)) {
                continue;
            }
            let n = st.len() as f64;
            let cls = class_level(r);
            let mean_cw = st.iter().filter_map(|h| h.carried_weight).sum::<f64>() / n;
            let mut rows: Vec<Vec<f64>> = Vec::new();
            for h in &st {
                let past: Vec<&Run> = hist.get(&h.horse_id).map(|v| v.iter().rev().filter(|x| x.date < dnum).collect()).unwrap_or_default();
                coverage.0 += 1;
                if !past.is_empty() {
                    coverage.1 += 1;
                }
                let last3: Vec<&&Run> = past.iter().take(3).collect();
                let avg = |f: &dyn Fn(&Run) -> Option<f64>| -> f64 {
                    let v: Vec<f64> = last3.iter().filter_map(|r| f(r)).collect();
                    if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
                };
                let figs: Vec<f64> = last3.iter().filter_map(|r| r.fig).collect();
                let fig_best = figs.iter().cloned().fold(f64::NAN, f64::max);
                let last = past.first();
                let js = jockey.get(&h.jockey).copied().unwrap_or_default();
                let ts = trainer.get(&h.trainer).copied().unwrap_or_default();
                let last_js = last.and_then(|l| jockey.get(&l.jockey)).copied().unwrap_or_default();
                let fin = |r: &Run| r.rank.map(|k| (k as f64 - 1.0) / (r.n.max(2) as f64 - 1.0));
                let same_venue: Vec<&&Run> = past.iter().filter(|x| x.venue == r.venue_code && x.surface == r.surface).collect();
                let same_dist: Vec<&&Run> = past.iter().filter(|x| x.surface == r.surface && (x.distance as i64 - r.distance as i64).abs() <= 200).collect();
                let top3_rate = |v: &Vec<&&Run>| logit_rate(v.iter().filter(|x| x.rank.map(|k| k <= 3).unwrap_or(false)).count() as f64, v.len() as f64, 0.25, 3.0);
                rows.push(vec![
                    fig_best,
                    avg(&|r| r.fig),
                    last.and_then(|r| r.fig).unwrap_or(f64::NAN),
                    -avg(&|r| r.l3f_ratio),
                    avg(&|r| match (r.pace, r.pos4) { (Some(p), Some(q)) => Some(p * (q - 0.5)), _ => None }),
                    avg(&|r| match (r.pos4, fin(r)) { (Some(q), Some(f)) => Some(q - f), _ => None }),
                    -avg(&|r| r.time_diff.map(|d| d.clamp(0.0, 3.0))),
                    -avg(&fin),
                    -avg(&|r| r.pop.map(|p| (p as f64 / r.n.max(1) as f64).ln())),
                    avg(&|r| match (r.pop, r.rank) { (Some(p), Some(k)) => Some((p as f64).ln() - (k as f64).ln()), _ => None }),
                    last.map(|l| cls - l.class).unwrap_or(f64::NAN),
                    logit_rate(js.top3, js.n, 0.21, 50.0),
                    logit_rate(ts.top3, ts.n, 0.21, 50.0),
                    logit_rate(js.win, js.n, 0.07, 50.0),
                    last.map(|l| ((ymd_days(dnum) - ymd_days(l.date)).max(1) as f64).ln()).unwrap_or(f64::NAN),
                    last.map(|l| ((r.distance as f64 - l.distance as f64).abs() / 400.0).min(3.0)).unwrap_or(f64::NAN),
                    last.map(|l| if l.surface != r.surface { 1.0 } else { 0.0 }).unwrap_or(f64::NAN),
                    h.carried_weight.unwrap_or(mean_cw) - mean_cw,
                    h.weight_diff.map(|d| (d.abs() as f64).min(30.0)).unwrap_or(f64::NAN),
                    h.body_weight.map(|b| b as f64).unwrap_or(f64::NAN),
                    h.age as f64,
                    (past.len() as f64 + 1.0).ln(),
                    if h.blinker { 1.0 } else { 0.0 },
                    if h.blinker && !past.iter().any(|x| x.blinker) { 1.0 } else { 0.0 },
                    h.umaban as f64 / n,
                    avg(&|r| if r.style > 0 { Some(r.style as f64) } else { None }),
                    if same_venue.is_empty() { f64::NAN } else { top3_rate(&same_venue) },
                    if same_dist.is_empty() { f64::NAN } else { top3_rate(&same_dist) },
                    last.map(|l| if l.jockey != h.jockey { 1.0 } else { 0.0 }).unwrap_or(f64::NAN),
                    last.map(|_| logit_rate(js.top3, js.n, 0.21, 50.0) - logit_rate(last_js.top3, last_js.n, 0.21, 50.0)).unwrap_or(f64::NAN),
                ]);
                // 調教: レース日の前日までの14日間。速いほど大きい値になるよう、基準との差を「基準 − タイム」で持つ
                let row = rows.last_mut().unwrap();
                let from = ymd_days(dnum) - 14;
                let recent: Vec<&Work> = works
                    .get(&h.horse_id)
                    .map(|v| v.iter().filter(|w| w.date < dnum && ymd_days(w.date) >= from).collect())
                    .unwrap_or_default();
                let rel = |w: &Work, four: bool| -> Option<f64> {
                    let (s4, s1) = work_std.get(&work_key(w))?;
                    if four { w.t4.map(|t| s4 - t) } else { w.last1.map(|t| s1 - t) }
                };
                for slope in [true, false] {
                    let ws: Vec<&&Work> = recent.iter().filter(|w| w.slope == slope).collect();
                    let best = |four: bool| ws.iter().filter_map(|w| rel(w, four)).fold(f64::NAN, f64::max);
                    row.push(best(true));
                    row.push(best(false));
                    row.push(ws.len() as f64);
                }
                let lastw = recent.last();
                row.push(lastw.map(|w| (ymd_days(dnum) - ymd_days(w.date)) as f64).unwrap_or(f64::NAN));
                row.push(lastw.and_then(|w| rel(w, false)).unwrap_or(f64::NAN));
                // 詳細: 最終追い切りの加速（1つ前のラップ − ラスト1F、正なら加速）
                row.push(lastw.and_then(|w| match (w.lap2, w.last1) { (Some(a), Some(b)) => Some((a - b).clamp(-3.0, 3.0)), _ => None }).unwrap_or(f64::NAN));
                // 自己比: 14日間の最速ラスト1F（基準との差）− 15〜120日前の同じ種類の中央値（基準との差）
                let own = |slope: bool| -> Option<f64> {
                    let mut v: Vec<f64> = works
                        .get(&h.horse_id)?
                        .iter()
                        .filter(|w| w.slope == slope && w.date < dnum && ymd_days(w.date) < from && ymd_days(w.date) >= ymd_days(dnum) - 120)
                        .filter_map(|w| rel(w, false))
                        .collect();
                    if v.len() < 3 { None } else { Some(median(&mut v)) }
                };
                let self_rel = lastw.and_then(|lw| {
                    let best = recent.iter().filter(|w| w.slope == lw.slope).filter_map(|w| rel(w, false)).fold(f64::NAN, f64::max);
                    own(lw.slope).filter(|_| best.is_finite()).map(|m| best - m)
                });
                row.push(self_rel.unwrap_or(f64::NAN));
                row.push(lastw.map(|w| if w.partners > 0 { 1.0 } else { 0.0 }).unwrap_or(f64::NAN));
                let from28 = ymd_days(dnum) - 28;
                row.push(works.get(&h.horse_id).map(|v| v.iter().filter(|w| w.date < dnum && ymd_days(w.date) >= from28).count() as f64).unwrap_or(0.0));
                row.push(lastw.map(|w| if w.slope { 1.0 } else { 0.0 }).unwrap_or(f64::NAN));
            }
            // 欠けた値はレース内の平均で埋め、レース内で標準化
            let k = rows[0].len();
            let mut cols: Vec<Vec<f64>> = (0..k).map(|c| rows.iter().map(|row| row[c]).collect()).collect();
            for col in cols.iter_mut() {
                let ok: Vec<f64> = col.iter().cloned().filter(|v| v.is_finite()).collect();
                let m = if ok.is_empty() { 0.0 } else { ok.iter().sum::<f64>() / ok.len() as f64 };
                for v in col.iter_mut() {
                    if !v.is_finite() {
                        *v = m;
                    }
                }
                standardize(col);
            }
            let inv: Vec<f64> = st.iter().map(|h| 1.0 / h.win_odds.unwrap()).collect();
            let s: f64 = inv.iter().sum();
            data.push((
                day.clone(),
                st.iter()
                    .enumerate()
                    .map(|(a, h)| Runner {
                        market: inv[a] / s,
                        odds: h.win_odds.unwrap(),
                        win: h.rank == Some(1),
                        x: (0..k).map(|c| cols[c][a]).collect(),
                    })
                    .collect(),
            ));
        }
        // 2) その日の結果を履歴に足す（翌日以降の特徴量にだけ効く）
        for r in today {
            let st: Vec<&JvRunner> = r.runners.iter().filter(|h| started(h)).collect();
            let n = st.len() as u32;
            let key = format!("{}|{}|{}", r.venue_code, r.track_code, r.distance);
            let ga = going_adj.get(&format!("{}|{}", r.surface, going(r))).copied().unwrap_or(0.0);
            let pace = match (r.first3f, r.last3f, pace_std.get(&format!("{}|{}", r.surface, r.distance))) {
                (Some(f), Some(l), Some(b)) => Some((f - l - b).clamp(-5.0, 5.0)),
                _ => None,
            };
            let mut l3: Vec<f64> = st.iter().filter_map(|h| h.last3f).collect();
            l3.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for h in &st {
                let fig = match (h.time_sec, std_t.get(&key)) {
                    (Some(t), Some(b)) if r.surface != "障" => Some(((b - t) / r.distance as f64 * 1000.0 + ga).clamp(-6.0, 6.0)),
                    _ => None,
                };
                let l3_ratio = h.last3f.map(|v| l3.iter().filter(|x| **x < v).count() as f64 / l3.len().max(1) as f64);
                hist.entry(h.horse_id.clone()).or_default().push(Run {
                    date: dnum,
                    venue: r.venue_code.clone(),
                    surface: r.surface.clone(),
                    distance: r.distance,
                    class: class_level(r),
                    rank: h.rank,
                    n,
                    time_diff: h.time_diff,
                    fig,
                    l3f_ratio: l3_ratio,
                    pos4: h.corners[3].map(|p| p as f64 / n.max(1) as f64),
                    pace,
                    pop: h.popularity,
                    style: h.style,
                    blinker: h.blinker,
                    jockey: h.jockey.clone(),
                });
                let w = if h.rank == Some(1) { 1.0 } else { 0.0 };
                let t = if h.rank.map(|k| k <= 3).unwrap_or(false) { 1.0 } else { 0.0 };
                for (m, key) in [(&mut jockey, &h.jockey), (&mut trainer, &h.trainer)] {
                    let e = m.entry(key.clone()).or_default();
                    e.n += 1.0;
                    e.win += w;
                    e.top3 += t;
                }
            }
        }
        i = j;
    }
    println!(
        "評価に使えるレース {} / 出走 {} 頭のうち過去走あり {:.1}%",
        data.len(),
        coverage.0,
        coverage.1 as f64 / coverage.0.max(1) as f64 * 100.0
    );
    evaluate(&data);
}

// ---- 当てはめと評価 ----

fn softmax(v: &[f64]) -> Vec<f64> {
    let m = v.iter().cloned().fold(f64::MIN, f64::max);
    let e: Vec<f64> = v.iter().map(|x| (x - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.iter().map(|x| x / s).collect()
}

/// 列の作り方: 先頭に log(市場) を入れるかどうかと、使う特徴量の番号
fn design(r: &Runner, with_market: bool, cols: &[usize]) -> Vec<f64> {
    let mut v = Vec::with_capacity(cols.len() + 1);
    if with_market {
        v.push(r.market.max(1e-9).ln());
    }
    v.extend(cols.iter().map(|&c| r.x[c]));
    v
}

/// 1着の多項ロジットをニュートン法で当てはめる（市場の係数以外に L2）
fn fit(train: &[&Vec<Runner>], with_market: bool, cols: &[usize], l2: f64) -> Vec<f64> {
    let k = cols.len() + with_market as usize;
    let mut b = vec![0.0; k];
    if with_market {
        b[0] = 1.0;
    }
    let n = train.len() as f64;
    for _ in 0..12 {
        let mut g = vec![0.0; k];
        let mut hm = vec![vec![0.0; k]; k];
        for race in train {
            let xs: Vec<Vec<f64>> = race.iter().map(|r| design(r, with_market, cols)).collect();
            let p = softmax(&xs.iter().map(|x| x.iter().zip(&b).map(|(a, c)| a * c).sum()).collect::<Vec<_>>());
            let w = race.iter().position(|r| r.win).unwrap();
            let mut ex = vec![0.0; k];
            for (pi, x) in p.iter().zip(&xs) {
                for a in 0..k {
                    ex[a] += pi * x[a];
                }
            }
            for a in 0..k {
                g[a] += xs[w][a] - ex[a];
            }
            for (pi, x) in p.iter().zip(&xs) {
                for a in 0..k {
                    let da = x[a] - ex[a];
                    for c in a..k {
                        hm[a][c] += pi * da * (x[c] - ex[c]);
                    }
                }
            }
        }
        for a in 0..k {
            for c in 0..a {
                hm[a][c] = hm[c][a];
            }
            let reg = if with_market && a == 0 { 0.0 } else { l2 };
            g[a] = g[a] / n - reg * b[a];
            for c in 0..k {
                hm[a][c] /= n;
            }
            hm[a][a] += reg;
        }
        let step = solve(hm, g);
        let mx = step.iter().map(|s| s.abs()).fold(0.0, f64::max);
        for a in 0..k {
            b[a] += step[a];
        }
        if mx < 1e-7 {
            break;
        }
    }
    b
}

fn solve(mut a: Vec<Vec<f64>>, mut y: Vec<f64>) -> Vec<f64> {
    let n = y.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap()).unwrap();
        a.swap(c, p);
        y.swap(c, p);
        let d = a[c][c];
        if d.abs() < 1e-12 {
            continue;
        }
        for r in 0..n {
            if r != c {
                let f = a[r][c] / d;
                if f != 0.0 {
                    for k in c..n {
                        a[r][k] -= f * a[c][k];
                    }
                    y[r] -= f * y[c];
                }
            }
        }
    }
    (0..n).map(|i| if a[i][i].abs() < 1e-12 { 0.0 } else { y[i] / a[i][i] }).collect()
}

fn race_probs(race: &[Runner], with_market: bool, cols: &[usize], b: &[f64]) -> Vec<f64> {
    softmax(&race.iter().map(|r| design(r, with_market, cols).iter().zip(b).map(|(a, c)| a * c).sum()).collect::<Vec<_>>())
}

fn loss(race: &[Runner], p: &[f64]) -> f64 {
    -p[race.iter().position(|r| r.win).unwrap()].max(1e-9).ln()
}

fn paired_ci(a: &[f64], b: &[f64]) -> (f64, f64, f64) {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    let n = d.len();
    let mean = d.iter().sum::<f64>() / n as f64;
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut ms: Vec<f64> = (0..2000).map(|_| (0..n).map(|_| d[(next() % n as u64) as usize]).sum::<f64>() / n as f64).collect();
    ms.sort_by(|x, y| x.partial_cmp(y).unwrap());
    (mean, ms[50], ms[1949])
}

fn evaluate(data: &[(String, Vec<Runner>)]) {
    let all: Vec<usize> = (0..NAMES.len()).collect();
    let mut variants: Vec<(String, bool, Vec<usize>)> = vec![
        ("市場(γ補正)".into(), true, vec![]),
        ("モデルのみ(市場なし)".into(), false, all.clone()),
        // 採用している 38 特徴量（調教の詳細 5 つは検証で悪化したので外す）
        ("市場+全特徴量".into(), true, (0..38).collect()),
        ("市場+全特徴量+調教詳細".into(), true, all.clone()),
    ];
    for (g, cols) in GROUPS {
        variants.push((format!("市場+{}", g), true, cols.to_vec()));
    }
    let folds = 8;
    let per = data.len() / folds;
    let mut pooled: Vec<Vec<f64>> = vec![Vec::new(); variants.len() + 1];
    let mut coefs: Vec<Vec<f64>> = Vec::new();
    const TH: [f64; 4] = [0.8, 0.9, 1.0, 1.1];
    let mut roi = [(0.0f64, 0.0f64); 4];
    println!("\n[時系列ローリング検証: 1着の対数損失]");
    for k in 4..folds {
        let train: Vec<&Vec<Runner>> = data[..k * per].iter().map(|(_, r)| r).collect();
        let test = &data[k * per..((k + 1) * per).min(data.len())];
        let lm: Vec<f64> = test.iter().map(|(_, r)| loss(r, &r.iter().map(|x| x.market).collect::<Vec<_>>())).collect();
        let mut line = format!("  {}〜 ({}R) 市場 {:.4}", test[0].0, test.len(), lm.iter().sum::<f64>() / lm.len() as f64);
        pooled[0].extend(lm);
        for (vi, (name, wm, cols)) in variants.iter().enumerate() {
            let b = fit(&train, *wm, cols, 0.01);
            let ls: Vec<f64> = test.iter().map(|(_, r)| loss(r, &race_probs(r, *wm, cols, &b))).collect();
            if vi <= 2 {
                line += &format!(" | {} {:.4}", name, ls.iter().sum::<f64>() / ls.len() as f64);
            }
            if vi == 2 {
                coefs.push(b.clone());
                for (_, race) in test {
                    let p = race_probs(race, true, cols, &b);
                    for (i, r) in race.iter().enumerate() {
                        for (ti, t) in TH.iter().enumerate() {
                            if p[i] * r.odds >= *t {
                                roi[ti].0 += 100.0;
                                if r.win {
                                    roi[ti].1 += r.odds * 100.0;
                                }
                            }
                        }
                    }
                }
            }
            pooled[vi + 1].extend(ls);
        }
        println!("{}", line);
    }
    let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    println!("\n[検証全区間 {} レースの平均] 市場 {:.4} / 市場(γ補正) {:.4} / モデルのみ {:.4} / 市場+全特徴量 {:.4}",
        pooled[0].len(), avg(&pooled[0]), avg(&pooled[1]), avg(&pooled[2]), avg(&pooled[3]));
    println!("\n[市場(γ補正)との対応のある比較（負なら良い, 95%区間）]");
    for vi in 2..variants.len() {
        let (m, lo, hi) = paired_ci(&pooled[vi + 1], &pooled[1]);
        println!("  {:<22} {:+.4} [{:+.4}, {:+.4}]{}", variants[vi].0, m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
    }
    {
        // pooled[0]=市場, [1]=市場(γ補正), [2]=モデルのみ, [3]=市場+全特徴量, [4]=+調教詳細
        let (m, lo, hi) = paired_ci(&pooled[4], &pooled[3]);
        println!("  調教詳細の上積み = (+調教詳細) − 全特徴量: {:+.4} [{:+.4}, {:+.4}]{}", m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
    }
    println!("\n[市場+全特徴量の係数（区間ごと。符号が揃うものが本物）]");
    println!("  {:<12} {}", "log(市場)", coefs.iter().map(|b| format!("{:+.3}", b[0])).collect::<Vec<_>>().join(" "));
    // 採用モデル（variants[2]）の列の順に表示する
    for (j, &f) in variants[2].2.iter().enumerate() {
        let name = NAMES[f];
        let v: Vec<f64> = coefs.iter().map(|b| b[j + 1]).collect();
        let same = v.iter().all(|x| *x > 0.0) || v.iter().all(|x| *x < 0.0);
        println!("  {:<12} {}{}", name, v.iter().map(|x| format!("{:+.3}", x)).collect::<Vec<_>>().join(" "), if same { "  ✓" } else { "" });
    }
    println!("\n[単勝の回収率（市場+全特徴量, 検証区間, 確定オッズ）]");
    for (ti, t) in TH.iter().enumerate() {
        println!("  期待値 ≥ {:.1}: {:.1}%（{} 点）", t, roi[ti].1 / roi[ti].0.max(1.0) * 100.0, (roi[ti].0 / 100.0) as u64);
    }
}
