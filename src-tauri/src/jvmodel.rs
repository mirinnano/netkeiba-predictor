//! JRA-VAN の履歴から勝率を出すモデル。バックテスト（examples/jv_model.rs）とアプリが同じコードを使う。
//!
//! - `Engine` は「ある日より前に分かっていたこと」だけを持つ。`features` はその日のレースの特徴量を作り、
//!   `add_day` でその日の結果を履歴に足す。バックテストは日付順にこの2つを交互に呼ぶ。
//! - 予測したいレースは `Card`（出馬表で分かること）で渡す。netkeiba の出馬表からも JRA-VAN のデータからも作れる。
//! - 基準タイム・ペース・調教の基準は 2021年のデータだけで作る（検証区間は 2023年以降）。
use crate::jvdata::{JvRace, JvRunner, JvWork};
use chrono::Datelike;
use std::collections::HashMap;

pub const NAMES: [&str; 50] = [
    "指数最高3", "指数平均3", "指数前走", "上がり順位", "展開不利", "4角から挽回", "着差", "着順率", "過去人気",
    "人気以上", "クラス変化", "騎手3着内", "調教師3着内", "騎手勝率", "間隔", "距離変更", "芝ダ替わり", "斤量差",
    "馬体重増減", "馬体重", "馬齢", "出走数", "ブリンカー", "初ブリンカー", "馬番位置", "脚質", "同場3着内", "同距離3着内",
    "乗り替わり", "騎手の格上げ",
    "坂路4F", "坂路ラスト1F", "坂路本数", "ウッド4F", "ウッドラスト1F", "ウッド本数", "最終追切からの日数", "最終追切ラスト1F",
    "最終追切の加速", "自己比ラスト1F", "併せ馬", "28日本数", "最終が坂路",
    "父の同条件", "母父の同条件", "父の馬場適性", "父×未経験条件",
    "当日の内外×馬番", "当日の前後×脚質",
    "道悪×道悪実績",
];

/// 採用している特徴量（調教の詳細 38〜42・血統 43〜46 は検証で効かなかったので使わない。当日の馬場の偏り 47・48 は 2026-11-14 に採用）
pub const ADOPTED: [usize; 40] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 35, 36, 37, 47, 48,
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
    trainer: String,
    /// 馬場状態（1 良, 2 稍重, 3 重, 4 不良, 0 不明）
    going: u8,
}

#[derive(Default, Clone, Copy)]
struct Stat {
    n: f64,
    win: f64,
    top3: f64,
}

#[derive(Clone)]
struct Work {
    date: u32,
    slope: bool,
    center: u8,
    course: u8,
    t4: Option<f64>,
    last1: Option<f64>,
    lap2: Option<f64>,
    time: u32,
    partners: u32,
}

/// 出馬表の段階で分かる、1頭分の情報
#[derive(Debug, Clone)]
pub struct CardRunner {
    pub horse_id: String,
    pub umaban: u32,
    pub jockey: String,
    /// 調教師コード。分からなければ空（そのときは履歴の最後の調教師を使う）
    pub trainer: String,
    pub carried_weight: Option<f64>,
    pub body_weight: Option<u32>,
    pub weight_diff: Option<i32>,
    pub age: u32,
    pub blinker: bool,
}

/// 予測するレース（出馬表で分かること）
#[derive(Debug, Clone)]
pub struct Card {
    pub date: u32,
    pub venue_code: String,
    pub surface: String,
    pub distance: u32,
    pub class: f64,
    /// その日のこのレースの馬場状態（1 良, 2 稍重, 3 重, 4 不良, 0 不明）
    pub going: u8,
    pub runners: Vec<CardRunner>,
}

pub fn class_level(condition_min: &str, grade: &str) -> f64 {
    let base = match condition_min {
        "701" | "702" | "703" => 0.0,
        "005" => 1.0,
        "010" => 2.0,
        "016" => 3.0,
        "999" => 4.0,
        _ => 1.0,
    };
    match grade {
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

pub fn ymd_days(d: u32) -> i64 {
    chrono::NaiveDate::from_ymd_opt((d / 10000) as i32, d / 100 % 100, d % 100)
        .map(|x| x.num_days_from_ce() as i64)
        .unwrap_or(0)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// 出走した馬（出走取消・発走除外・競走除外を除く）
pub fn started(h: &JvRunner) -> bool {
    !matches!(h.abnormal.as_str(), "1" | "2" | "3")
}

fn logit_rate(hits: f64, n: f64, prior: f64, k: f64) -> f64 {
    let p = (hits + prior * k) / (n + k);
    (p / (1.0 - p)).ln()
}

fn work_key(w: &Work) -> (bool, u8, u8) {
    (w.slope, w.center, if w.slope { 0 } else { w.course })
}

/// JRA-VAN の確定レースを Card にする（出走した馬だけ）
pub fn card_from_jv(r: &JvRace) -> Card {
    Card {
        date: r.date.parse().unwrap_or(0),
        venue_code: r.venue_code.clone(),
        surface: r.surface.clone(),
        distance: r.distance,
        class: class_level(&r.condition_min, &r.grade),
        going: going(r).parse().unwrap_or(0),
        runners: r
            .runners
            .iter()
            .filter(|h| started(h))
            .map(|h| CardRunner {
                horse_id: h.horse_id.clone(),
                umaban: h.umaban,
                jockey: h.jockey.clone(),
                trainer: h.trainer.clone(),
                carried_weight: h.carried_weight,
                body_weight: h.body_weight,
                weight_diff: h.weight_diff,
                age: h.age,
                blinker: h.blinker,
            })
            .collect(),
    }
}

pub struct Engine {
    hist: HashMap<String, Vec<Run>>,
    jockey: HashMap<String, Stat>,
    trainer: HashMap<String, Stat>,
    works: HashMap<String, Vec<Work>>,
    std_t: HashMap<String, f64>,
    going_adj: HashMap<String, f64>,
    pace_std: HashMap<String, f64>,
    work_std: HashMap<(bool, u8, u8), (f64, f64)>,
    /// 血統（馬 → (父, 母父) の繁殖登録番号）。なければ血統の特徴量は欠け（NaN）
    pedigree: HashMap<String, (String, String)>,
    /// 父・母父ごとの産駒成績。キーは (種牡馬, 馬場, 距離帯)。馬場・距離帯が空のものは全体
    sire_stat: HashMap<(String, String, u8), Stat>,
    /// 当日の馬場の偏り（内外, 前後）。同じ日・同じ場・同じ馬場で先に終わったレースから作る。分からなければ NaN
    day_bias: (f64, f64),
}

/// 当日の馬場の偏りを測るための、先に終わったレースの 1 頭分（馬番・着順・最後のコーナー順位）
#[derive(Debug, Clone, Copy)]
pub struct DayRow {
    pub umaban: u32,
    pub rank: u32,
    pub corner_last: Option<u32>,
}

/// JRA-VAN の確定レースから DayRow を作る（出走して着順がある馬だけ）
pub fn day_rows(r: &JvRace) -> Vec<DayRow> {
    r.runners
        .iter()
        .filter(|h| started(h))
        .filter_map(|h| h.rank.map(|k| DayRow { umaban: h.umaban, rank: k, corner_last: h.corners[3] }))
        .collect()
}

pub fn day_bias(earlier: &[&JvRace]) -> (f64, f64) {
    day_bias_rows(&earlier.iter().map(|r| day_rows(r)).collect::<Vec<_>>())
}

/// 当日の馬場の偏り。先に終わったレース（2 つ以上）の結果から
/// 内外 = 内 1/3 の平均着順率 − 外 1/3 の平均着順率、前後 = 最後のコーナーで前 1/3 の平均着順率 − 後ろ 1/3 の平均着順率（負なら内・前が有利）
pub fn day_bias_rows(earlier: &[Vec<DayRow>]) -> (f64, f64) {
    if earlier.len() < 2 {
        return (f64::NAN, f64::NAN);
    }
    let (mut ins, mut outs, mut fr, mut bk) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for st in earlier {
        let n = st.len() as f64;
        if n < 5.0 {
            continue;
        }
        for h in st {
            let fin = (h.rank as f64 - 1.0) / (n - 1.0);
            let draw = h.umaban as f64 / n;
            if draw <= 1.0 / 3.0 { ins.push(fin) } else if draw > 2.0 / 3.0 { outs.push(fin) }
            if let Some(c) = h.corner_last {
                let q = c as f64 / n;
                if q <= 1.0 / 3.0 { fr.push(fin) } else if q > 2.0 / 3.0 { bk.push(fin) }
            }
        }
    }
    let mean = |v: &Vec<f64>| if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 };
    (mean(&ins) - mean(&outs), mean(&fr) - mean(&bk))
}

/// 距離帯: 〜1400 / 〜1800 / 〜2200 / それ以上
fn dist_band(d: u32) -> u8 {
    match d {
        0..=1400 => 1,
        1401..=1800 => 2,
        1801..=2200 => 3,
        _ => 4,
    }
}

impl Engine {
    /// 基準タイム・ペースは、渡されたレースのうち最初の年のものだけで作る（2021年から読めば 2021年、2011年から読めば 2011年）。
    /// 調教の基準も、調教データの最初の年の分だけで作る。評価する年より後の情報が基準に混ざらないようにするため
    pub fn new(base_races: &[JvRace], works: HashMap<String, Vec<JvWork>>) -> Self {
        let first_year = base_races.iter().filter(|r| r.kubun == '7').map(|r| r.date.get(0..4).unwrap_or("9999")).min().unwrap_or("2021").to_string();
        let race_cut = format!("{}0101", first_year.parse::<u32>().unwrap_or(2021) + 1);
        let base: Vec<&JvRace> = base_races.iter().filter(|r| r.date.as_str() < race_cut.as_str() && r.surface != "障" && r.kubun == '7').collect();
        let mut tv: HashMap<String, Vec<f64>> = HashMap::new();
        let mut pv: HashMap<String, Vec<f64>> = HashMap::new();
        for r in &base {
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
        for r in &base {
            let key = format!("{}|{}|{}", r.venue_code, r.track_code, r.distance);
            if let Some(b) = std_t.get(&key) {
                for h in &r.runners {
                    if let Some(t) = h.time_sec {
                        gv.entry(format!("{}|{}", r.surface, going(r))).or_default().push((t - b) / r.distance as f64 * 1000.0);
                    }
                }
            }
        }
        let going_adj = gv.into_iter().map(|(k, mut v)| (k, median(&mut v))).collect();
        let pace_std = pv.into_iter().filter(|(_, v)| v.len() >= 20).map(|(k, mut v)| (k, median(&mut v))).collect();

        let mut wm: HashMap<String, Vec<Work>> = HashMap::new();
        for (horse, v) in works {
            wm.insert(
                horse,
                v.into_iter()
                    .map(|w| Work { date: w.date, slope: w.slope, center: w.center, course: w.course, t4: w.t4, last1: w.last1, lap2: w.lap2, time: w.time, partners: 0 })
                    .collect(),
            );
        }
        let mut slots: HashMap<(bool, u8, u8, u32, u32), u32> = HashMap::new();
        for v in wm.values() {
            for w in v {
                *slots.entry((w.slope, w.center, w.course, w.date, w.time)).or_default() += 1;
            }
        }
        // 調教の基準は、種類（坂路・ウッドとトレセン・コース）ごとに、レースの基準年以降で最初にデータがある年の分だけで作る
        // （ウッドは 2021年7月からしかないので、2011年から読んでもウッドの基準は 2021年になる）
        let race_first: u32 = first_year.parse().unwrap_or(2021);
        let mut key_first: HashMap<(bool, u8, u8), u32> = HashMap::new();
        for v in wm.values() {
            for w in v.iter().filter(|w| w.date / 10000 >= race_first) {
                let e = key_first.entry(work_key(w)).or_insert(w.date / 10000);
                *e = (*e).min(w.date / 10000);
            }
        }
        let mut wv: HashMap<(bool, u8, u8), (Vec<f64>, Vec<f64>)> = HashMap::new();
        for v in wm.values_mut() {
            for w in v.iter_mut() {
                w.partners = slots.get(&(w.slope, w.center, w.course, w.date, w.time)).copied().unwrap_or(1) - 1;
                if key_first.get(&work_key(w)).map(|y| w.date / 10000 == *y).unwrap_or(false) {
                    let e = wv.entry(work_key(w)).or_default();
                    if let Some(t) = w.t4 {
                        e.0.push(t);
                    }
                    if let Some(t) = w.last1 {
                        e.1.push(t);
                    }
                }
            }
            v.sort_by_key(|w| (w.date, w.time));
        }
        let work_std = wv
            .into_iter()
            .filter(|(_, (a, b))| a.len() >= 100 && b.len() >= 100)
            .map(|(k, (mut a, mut b))| (k, (median(&mut a), median(&mut b))))
            .collect();
        Engine { hist: HashMap::new(), jockey: HashMap::new(), trainer: HashMap::new(), works: wm, std_t, going_adj, pace_std, work_std, pedigree: HashMap::new(), sire_stat: HashMap::new(), day_bias: (f64::NAN, f64::NAN) }
    }

    pub fn summary(&self) -> String {
        format!(
            "基準タイム {} 条件 / ペース基準 {} 条件 / 調教 {} 頭・基準 {} 種類 / 履歴 {} 頭",
            self.std_t.len(),
            self.pace_std.len(),
            self.works.len(),
            self.work_std.len(),
            self.hist.len()
        )
    }

    /// 血統を設定する（馬 → (父, 母父)）。履歴を足す前に呼ぶ
    pub fn set_pedigree(&mut self, pedigree: HashMap<String, (String, String)>) {
        self.pedigree = pedigree;
    }

    /// 当日の馬場の偏りを設定する（次に作る出馬表の特徴量に使う）。分からなければ (NaN, NaN)
    pub fn set_day_bias(&mut self, b: (f64, f64)) {
        self.day_bias = b;
    }

    /// その馬に過去走があるか
    pub fn has_history(&self, horse_id: &str, date: u32) -> bool {
        self.hist.get(horse_id).map(|v| v.iter().any(|r| r.date < date)).unwrap_or(false)
    }

    /// 確定レースの結果を履歴に足す（同じ日のレースをまとめて渡す）
    pub fn add_day(&mut self, races: &[JvRace]) {
        for r in races.iter().filter(|r| r.kubun == '7') {
            let dnum: u32 = r.date.parse().unwrap_or(0);
            let st: Vec<&JvRunner> = r.runners.iter().filter(|h| started(h)).collect();
            let n = st.len() as u32;
            let key = format!("{}|{}|{}", r.venue_code, r.track_code, r.distance);
            let ga = self.going_adj.get(&format!("{}|{}", r.surface, going(r))).copied().unwrap_or(0.0);
            let pace = match (r.first3f, r.last3f, self.pace_std.get(&format!("{}|{}", r.surface, r.distance))) {
                (Some(f), Some(l), Some(b)) => Some((f - l - b).clamp(-5.0, 5.0)),
                _ => None,
            };
            let mut l3: Vec<f64> = st.iter().filter_map(|h| h.last3f).collect();
            l3.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for h in &st {
                let fig = match (h.time_sec, self.std_t.get(&key)) {
                    (Some(t), Some(b)) if r.surface != "障" => Some(((b - t) / r.distance as f64 * 1000.0 + ga).clamp(-6.0, 6.0)),
                    _ => None,
                };
                let l3_ratio = h.last3f.map(|v| l3.iter().filter(|x| **x < v).count() as f64 / l3.len().max(1) as f64);
                self.hist.entry(h.horse_id.clone()).or_default().push(Run {
                    date: dnum,
                    venue: r.venue_code.clone(),
                    surface: r.surface.clone(),
                    distance: r.distance,
                    class: class_level(&r.condition_min, &r.grade),
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
                    trainer: h.trainer.clone(),
                    going: going(r).parse().unwrap_or(0),
                });
                let w = if h.rank == Some(1) { 1.0 } else { 0.0 };
                let t = if h.rank.map(|k| k <= 3).unwrap_or(false) { 1.0 } else { 0.0 };
                for (m, k) in [(&mut self.jockey, &h.jockey), (&mut self.trainer, &h.trainer)] {
                    let e = m.entry(k.clone()).or_default();
                    e.n += 1.0;
                    e.win += w;
                    e.top3 += t;
                }
                if let Some((sire, bms)) = self.pedigree.get(&h.horse_id) {
                    for anc in [format!("S{}", sire), format!("B{}", bms)] {
                        for key in [(anc.clone(), r.surface.clone(), dist_band(r.distance)), (anc.clone(), r.surface.clone(), 0), (anc.clone(), String::new(), 0)] {
                            let e = self.sire_stat.entry(key).or_default();
                            e.n += 1.0;
                            e.win += w;
                            e.top3 += t;
                        }
                    }
                }
            }
        }
    }

    /// 出走馬ごとの特徴量（標準化前、43列。欠けは NaN）。card.date より前の情報だけを使う
    pub fn raw_features(&self, card: &Card) -> Vec<Vec<f64>> {
        let dnum = card.date;
        let n = card.runners.len() as f64;
        let cws: Vec<f64> = card.runners.iter().filter_map(|h| h.carried_weight).collect();
        let mean_cw = if cws.is_empty() { 0.0 } else { cws.iter().sum::<f64>() / cws.len() as f64 };
        let empty: Vec<Run> = Vec::new();
        card.runners
            .iter()
            .map(|h| {
                let past: Vec<&Run> = self.hist.get(&h.horse_id).unwrap_or(&empty).iter().rev().filter(|x| x.date < dnum).collect();
                let last3: Vec<&&Run> = past.iter().take(3).collect();
                let avg = |f: &dyn Fn(&Run) -> Option<f64>| -> f64 {
                    let v: Vec<f64> = last3.iter().filter_map(|r| f(r)).collect();
                    if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 }
                };
                let fig_best = last3.iter().filter_map(|r| r.fig).fold(f64::NAN, f64::max);
                let last = past.first();
                let trainer = if h.trainer.is_empty() { last.map(|l| l.trainer.clone()).unwrap_or_default() } else { h.trainer.clone() };
                let js = self.jockey.get(&h.jockey).copied().unwrap_or_default();
                let ts = self.trainer.get(&trainer).copied().unwrap_or_default();
                let last_js = last.and_then(|l| self.jockey.get(&l.jockey)).copied().unwrap_or_default();
                let fin = |r: &Run| r.rank.map(|k| (k as f64 - 1.0) / (r.n.max(2) as f64 - 1.0));
                let same_venue: Vec<&&Run> = past.iter().filter(|x| x.venue == card.venue_code && x.surface == card.surface).collect();
                let same_dist: Vec<&&Run> =
                    past.iter().filter(|x| x.surface == card.surface && (x.distance as i64 - card.distance as i64).abs() <= 200).collect();
                let top3_rate = |v: &Vec<&&Run>| {
                    logit_rate(v.iter().filter(|x| x.rank.map(|k| k <= 3).unwrap_or(false)).count() as f64, v.len() as f64, 0.25, 3.0)
                };
                let mut row = vec![
                    fig_best,
                    avg(&|r| r.fig),
                    last.and_then(|r| r.fig).unwrap_or(f64::NAN),
                    -avg(&|r| r.l3f_ratio),
                    avg(&|r| match (r.pace, r.pos4) {
                        (Some(p), Some(q)) => Some(p * (q - 0.5)),
                        _ => None,
                    }),
                    avg(&|r| match (r.pos4, fin(r)) {
                        (Some(q), Some(f)) => Some(q - f),
                        _ => None,
                    }),
                    -avg(&|r| r.time_diff.map(|d| d.clamp(0.0, 3.0))),
                    -avg(&fin),
                    -avg(&|r| r.pop.map(|p| (p as f64 / r.n.max(1) as f64).ln())),
                    avg(&|r| match (r.pop, r.rank) {
                        (Some(p), Some(k)) => Some((p as f64).ln() - (k as f64).ln()),
                        _ => None,
                    }),
                    last.map(|l| card.class - l.class).unwrap_or(f64::NAN),
                    logit_rate(js.top3, js.n, 0.21, 50.0),
                    logit_rate(ts.top3, ts.n, 0.21, 50.0),
                    logit_rate(js.win, js.n, 0.07, 50.0),
                    last.map(|l| ((ymd_days(dnum) - ymd_days(l.date)).max(1) as f64).ln()).unwrap_or(f64::NAN),
                    last.map(|l| ((card.distance as f64 - l.distance as f64).abs() / 400.0).min(3.0)).unwrap_or(f64::NAN),
                    last.map(|l| if l.surface != card.surface { 1.0 } else { 0.0 }).unwrap_or(f64::NAN),
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
                ];
                // 調教: レース日の前日までの14日間。速いほど大きい値になるよう「基準 − タイム」で持つ
                let from = ymd_days(dnum) - 14;
                let all_w: &[Work] = self.works.get(&h.horse_id).map(|v| v.as_slice()).unwrap_or(&[]);
                let recent: Vec<&Work> = all_w.iter().filter(|w| w.date < dnum && ymd_days(w.date) >= from).collect();
                let rel = |w: &Work, four: bool| -> Option<f64> {
                    let (s4, s1) = self.work_std.get(&work_key(w))?;
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
                // 調教の詳細（不採用だが、バックテストで比べるために作る）
                row.push(lastw.and_then(|w| match (w.lap2, w.last1) {
                    (Some(a), Some(b)) => Some((a - b).clamp(-3.0, 3.0)),
                    _ => None,
                }).unwrap_or(f64::NAN));
                let own = |slope: bool| -> Option<f64> {
                    let mut v: Vec<f64> = all_w
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
                row.push(all_w.iter().filter(|w| w.date < dnum && ymd_days(w.date) >= from28).count() as f64);
                row.push(lastw.map(|w| if w.slope { 1.0 } else { 0.0 }).unwrap_or(f64::NAN));
                // 血統: 父・母父の産駒の、同じ馬場・距離帯での3着内率（前日までの成績）
                let band = dist_band(card.distance);
                let rate = |anc: &str, surf: &str, b: u8| -> f64 {
                    match self.sire_stat.get(&(anc.to_string(), surf.to_string(), b)) {
                        Some(st) if st.n > 0.0 => logit_rate(st.top3, st.n, 0.21, 30.0),
                        _ => f64::NAN,
                    }
                };
                match self.pedigree.get(&h.horse_id) {
                    Some((sire, bms)) => {
                        let (s, b) = (format!("S{}", sire), format!("B{}", bms));
                        let same = rate(&s, &card.surface, band);
                        row.push(same);
                        row.push(rate(&b, &card.surface, band));
                        row.push(rate(&s, &card.surface, 0) - rate(&s, "", 0));
                        let tried = past.iter().any(|x| x.surface == card.surface && dist_band(x.distance) == band);
                        row.push(if tried { 0.0 } else { same - rate(&s, "", 0) });
                    }
                    None => row.extend([f64::NAN; 4]),
                }
                // 当日の馬場の偏り × この馬の位置（馬番位置、過去 3 走の 4 角位置）
                row.push(self.day_bias.0 * (h.umaban as f64 / n - 0.5));
                let pos = avg(&|r| r.pos4);
                row.push(self.day_bias.1 * (pos - 0.5));
                // 道悪×道悪実績: その日が稍重以下のときだけ、同じ馬場の道悪での 3 着内率 − 全体の 3 着内率
                row.push(if card.going >= 2 {
                    let surf: Vec<&&Run> = past.iter().filter(|x| x.surface == card.surface).collect();
                    let wet: Vec<&&Run> = surf.iter().copied().filter(|x| x.going >= 2).collect();
                    if wet.is_empty() { f64::NAN } else { top3_rate(&wet) - top3_rate(&surf) }
                } else {
                    0.0
                });
                row
            })
            .collect()
    }

    /// 欠けた値をレース内の平均で埋め、列ごとにレース内で標準化した特徴量
    pub fn features(&self, card: &Card) -> Vec<Vec<f64>> {
        standardize_rows(self.raw_features(card))
    }
}

pub fn standardize_rows(rows: Vec<Vec<f64>>) -> Vec<Vec<f64>> {
    if rows.is_empty() {
        return rows;
    }
    let k = rows[0].len();
    let n = rows.len();
    let mut out = rows;
    for c in 0..k {
        let ok: Vec<f64> = out.iter().map(|r| r[c]).filter(|v| v.is_finite()).collect();
        let fill = if ok.is_empty() { 0.0 } else { ok.iter().sum::<f64>() / ok.len() as f64 };
        for r in out.iter_mut() {
            if !r[c].is_finite() {
                r[c] = fill;
            }
        }
        let m = out.iter().map(|r| r[c]).sum::<f64>() / n as f64;
        let sd = (out.iter().map(|r| (r[c] - m).powi(2)).sum::<f64>() / n as f64).sqrt();
        for r in out.iter_mut() {
            r[c] = if sd > 1e-9 { (r[c] - m) / sd } else { 0.0 };
        }
    }
    out
}

fn softmax(v: &[f64]) -> Vec<f64> {
    let m = v.iter().cloned().fold(f64::MIN, f64::max);
    let e: Vec<f64> = v.iter().map(|x| (x - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.iter().map(|x| x / s).collect()
}

/// 採用特徴量（ADOPTED の順）が、表示用のどの要素に入るか（models::FACTOR_NAMES の 0..5）
/// 0=能力 1=時計 2=適性 3=騎手・枠 4=展開 5=調教
pub const JV_GROUP: [usize; 40] = [
    1, 1, 1, 1, 4, 4, 0, 0, 0, 0, 0, 3, 3, 3, 0, 2, 2, 3, 0, 0, 0, 0, 0, 0, 3, 4, 2, 2, 3, 3,
    5, 5, 5, 5, 5, 5, 5, 5, 3, 4,
];

/// 勝率。全頭に単勝オッズがあれば「市場＋モデル」、なければ「モデルのみ」の係数を使う。
/// 係数は src/jv_coef.rs（`jv_model --export` が書き出す）
pub fn win_probs(features: &[Vec<f64>], odds: &[Option<f64>]) -> (Vec<f64>, bool) {
    let (p, m, _) = win_probs_detail(features, odds);
    (p, m)
}

/// 勝率に加えて、馬ごとの要素別の寄与を返す。
/// 寄与は「オッズを使わないモデル」の係数で出す（市場モデルの係数はオッズに対する上積みなので、
/// 能力などの絶対的な強さとしては読めないため）。7番目（単勝オッズ）は常に 0。
pub fn win_probs_detail(features: &[Vec<f64>], odds: &[Option<f64>]) -> (Vec<f64>, bool, Vec<[f64; 7]>) {
    use crate::jv_coef::{MARKET_BETA, MARKET_GAMMA, SOLO_BETA};
    let with_market = !odds.is_empty() && odds.iter().all(|o| o.map(|v| v > 0.0).unwrap_or(false));
    let u: Vec<f64> = if with_market {
        let inv: Vec<f64> = odds.iter().map(|o| 1.0 / o.unwrap()).collect();
        let s: f64 = inv.iter().sum();
        features
            .iter()
            .zip(&inv)
            .map(|(x, iv)| MARKET_GAMMA * (iv / s).ln() + ADOPTED.iter().zip(MARKET_BETA.iter()).map(|(&f, b)| b * x[f]).sum::<f64>())
            .collect()
    } else {
        features.iter().map(|x| ADOPTED.iter().zip(SOLO_BETA.iter()).map(|(&f, b)| b * x[f]).sum::<f64>()).collect()
    };
    let contrib: Vec<[f64; 7]> = features
        .iter()
        .map(|x| {
            let mut c = [0.0; 7];
            for (j, (&f, b)) in ADOPTED.iter().zip(SOLO_BETA.iter()).enumerate() {
                c[JV_GROUP[j]] += b * x[f];
            }
            c
        })
        .collect();
    (softmax(&u), with_market, contrib)
}

/// 履歴を読み始める日。係数（jv_coef.rs）を当てはめたときと揃える（基準タイムはこの年のレースで作る）
pub const HISTORY_FROM: &str = "20110101";

/// アプリ用: `until`（yyyymmdd）の前日までの確定成績で Engine を作る
pub fn build_engine(conn: &rusqlite::Connection, until: u32) -> rusqlite::Result<Engine> {
    let until_s = until.to_string();
    let mut races = crate::jvstore::load_races(conn, HISTORY_FROM, &until_s)?;
    races.retain(|r| r.kubun == '7');
    races.sort_by(|a, b| (&a.date, &a.race_id).cmp(&(&b.date, &b.race_id)));
    let recent_from = {
        let d = chrono::NaiveDate::from_ymd_opt((until / 10000) as i32, until / 100 % 100, until % 100)
            .map(|d| d - chrono::Duration::days(150))
            .map(|d| d.format("%Y%m%d").to_string().parse().unwrap_or(0))
            .unwrap_or(0);
        d
    };
    let works = crate::jvstore::load_works_for(conn, recent_from)?;
    let mut engine = Engine::new(&races, works);
    let mut i = 0;
    while i < races.len() {
        let mut j = i;
        while j < races.len() && races[j].date == races[i].date {
            j += 1;
        }
        engine.add_day(&races[i..j]);
        i = j;
    }
    Ok(engine)
}
