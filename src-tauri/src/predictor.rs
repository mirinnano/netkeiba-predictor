use crate::models::{
    combo_key, BetRecommendation, ComboOdds, HorseEntry, HorsePrediction, OfficialPayoutItem,
    PaceAnalysis, RaceInfo, RacePredictionResult, ResultRow, ScoringWeights,
};

// JRA の払戻率。単勝 80%、馬連・ワイド 77.5%、3連複 75%。
const TAKEOUT_WIN: f64 = 0.80;
const TAKEOUT_UMAREN: f64 = 0.775;
const TAKEOUT_WIDE: f64 = 0.775;
const TAKEOUT_TRIO: f64 = 0.75;

/// Harville 型の着順モデル。勝率 p から着順の同時確率を出す。
///
/// 素の Harville は「2着・3着も勝率の比で決まる」と仮定するため、人気馬の2・3着を過大に見積もる。
/// Lo, Bacon-Shone & Busche (1995) の補正に従い、2着の段では p^λ2、3着の段では p^λ3 を
/// 残りの馬で正規化して使う。λ は 2024 年の JRA 391 レースで当てはめた値（backtest 参照）。
pub mod harville {
    pub const LAMBDA2: f64 = 0.90;
    pub const LAMBDA3: f64 = 0.70;

    pub fn normalize(v: &[f64]) -> Vec<f64> {
        let sum: f64 = v.iter().sum();
        if sum <= 0.0 {
            return vec![1.0 / v.len().max(1) as f64; v.len()];
        }
        v.iter().map(|x| x / sum).collect()
    }

    pub struct Model {
        p: Vec<f64>,
        q2: Vec<f64>,
        q3: Vec<f64>,
    }

    fn rest(x: f64) -> f64 {
        x.max(1e-12)
    }

    impl Model {
        /// p は合計 1 の勝率
        pub fn new(p: &[f64]) -> Self {
            Self::with_lambda(p, LAMBDA2, LAMBDA3)
        }

        pub fn with_lambda(p: &[f64], l2: f64, l3: f64) -> Self {
            Model {
                p: p.to_vec(),
                q2: normalize(&p.iter().map(|x| x.max(1e-12).powf(l2)).collect::<Vec<_>>()),
                q3: normalize(&p.iter().map(|x| x.max(1e-12).powf(l3)).collect::<Vec<_>>()),
            }
        }

        /// i が1着、j が2着
        pub fn exacta(&self, i: usize, j: usize) -> f64 {
            self.p[i] * self.q2[j] / rest(1.0 - self.q2[i])
        }

        /// i, j, k の順で1-2-3着
        pub fn trifecta(&self, i: usize, j: usize, k: usize) -> f64 {
            self.exacta(i, j) * self.q3[k] / rest(1.0 - self.q3[i] - self.q3[j])
        }

        /// 馬連: i と j が1-2着（順不同）
        pub fn quinella(&self, i: usize, j: usize) -> f64 {
            self.exacta(i, j) + self.exacta(j, i)
        }

        /// 3連複: i, j, k が1-2-3着（順不同）
        pub fn trio(&self, i: usize, j: usize, k: usize) -> f64 {
            self.trifecta(i, j, k)
                + self.trifecta(i, k, j)
                + self.trifecta(j, i, k)
                + self.trifecta(j, k, i)
                + self.trifecta(k, i, j)
                + self.trifecta(k, j, i)
        }

        /// ワイド: i と j がともに3着以内
        pub fn wide(&self, i: usize, j: usize) -> f64 {
            (0..self.p.len())
                .filter(|&k| k != i && k != j)
                .map(|k| self.trio(i, j, k))
                .sum()
        }

        /// 各馬の3着以内率
        pub fn top3(&self) -> Vec<f64> {
            let n = self.p.len();
            let mut out = vec![0.0; n];
            for i in 0..n {
                for j in 0..n {
                    if j == i {
                        continue;
                    }
                    for k in 0..n {
                        if k == i || k == j {
                            continue;
                        }
                        let pr = self.trifecta(i, j, k);
                        out[i] += pr;
                        out[j] += pr;
                        out[k] += pr;
                    }
                }
            }
            out
        }
    }
}

/// 勝率モデルの特徴量。5項目スコアと過去走から作り、レース内で標準化してから使う。
pub const FEATURE_NAMES: [&str; N_FEATURES] = [
    "能力", "時計", "適性", "騎手・枠", "展開", "着差", "過去人気", "着順率", "出走数", "斤量差",
    "指数(最高)", "指数(平均)", "間隔", "距離変更", "芝ダ替わり", "騎手3着内率", "父の芝ダ3着内率",
];
pub const N_FEATURES: usize = 17;
/// 各特徴量がどの重みスライダー（0=能力 1=時計 2=適性 3=騎手・枠 4=展開）に属するか
pub const FEATURE_GROUP: [usize; N_FEATURES] = [0, 1, 2, 3, 4, 0, 0, 0, 0, 3, 1, 1, 0, 2, 2, 3, 2];
/// 条件付きロジット（1着の多項ロジット）の係数。2023〜2025 年の JRA 1518 レースで当てはめた（L2 正則化 0.2）。
/// 時系列ローリング検証（189レース×4区間）の1着対数損失は平均 2.170
/// （5項目＋過去走だけなら 2.201、単勝オッズそのものは 1.875）。
/// 間隔・距離変更・芝ダ替わりは検証で効かず、展開は係数が負になったので 0。
/// 再学習: `cargo run --release --example backtest -- <races.jsonl>... --write-std src/std_times.rs` の出力を貼る。
pub const MODEL_BETA: [f64; N_FEATURES] = [0.165, 0.135, 0.167, 0.061, 0.0, -0.082, -0.165, -0.121, -0.055, -0.001, 0.101, 0.098, 0.0, 0.0, 0.0, 0.246, 0.217];

/// 過去走から作る特徴量: [着差, 過去人気, 着順率, 出走数, 斤量差]
pub fn past_features(e: &HorseEntry, field_carried_weight: f64) -> [f64; 5] {
    let runs: Vec<_> = e.past_runs.iter().take(5).collect();
    let rw = [1.0, 0.8, 0.6, 0.45, 0.35];
    let wavg = |f: &dyn Fn(&crate::models::PastRun) -> Option<f64>, default: f64| -> f64 {
        let (mut s, mut w) = (0.0, 0.0);
        for (i, r) in runs.iter().enumerate() {
            if let Some(v) = f(r) {
                s += v * rw[i];
                w += rw[i];
            }
        }
        if w > 0.0 {
            s / w
        } else {
            default
        }
    };
    // 勝ち馬との着差（秒）。勝った走りは 0、大敗は 3 秒で頭打ち
    let margin = wavg(&|r| r.margin.map(|m| m.clamp(0.0, 3.0)), 1.5);
    // 過去の人気: log(人気/頭数)。小さいほど支持されていた
    let pop = wavg(
        &|r| match (r.popularity, r.head_count) {
            (Some(p), Some(h)) if h > 0 => Some(((p as f64) / (h as f64)).ln()),
            _ => None,
        },
        -0.7,
    );
    // 着順率（0 が1着、1 が最下位）
    let fin = wavg(
        &|r| match (r.finish_rank, r.head_count) {
            (Some(f), Some(h)) if h > 1 => Some((f as f64 - 1.0) / (h as f64 - 1.0)),
            _ => None,
        },
        0.5,
    );
    [margin, pop, fin, runs.len() as f64, e.carried_weight - field_carried_weight]
}

/// コース・距離ごとの基準タイムと、馬場状態による補正。過去走のタイムを「1000m あたり何秒速いか」に直す。
pub struct StdTimes {
    base: std::collections::HashMap<String, f64>,
    going: std::collections::HashMap<String, f64>,
    /// 騎手（フルネーム）→ (騎乗数, 3着以内数)
    jockey: std::collections::HashMap<String, (u32, u32)>,
    /// 「父|芝ダ」→ (出走数, 3着以内数)。確定レースの結果から作る
    sire: std::collections::HashMap<String, (u32, u32)>,
}

fn std_key(r: &crate::models::PastRun) -> String {
    format!("{}|{}|{}", r.venue, r.surface, r.distance)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

impl StdTimes {
    /// 過去走の集まりから基準タイム（中央値）を作る。8 走未満のコースは使わない。
    pub fn fit<'a>(runs: impl Iterator<Item = &'a crate::models::PastRun>) -> Self {
        let runs: Vec<&crate::models::PastRun> = runs
            .filter(|r| r.time_sec.is_some() && r.distance >= 800 && r.surface != "障")
            .collect();
        let mut by: std::collections::HashMap<String, Vec<f64>> = Default::default();
        for r in &runs {
            if r.track_cond == "良" {
                by.entry(std_key(r)).or_default().push(r.time_sec.unwrap());
            }
        }
        let base: std::collections::HashMap<String, f64> = by
            .into_iter()
            .filter(|(_, v)| v.len() >= 8)
            .map(|(k, mut v)| (k, median(&mut v)))
            .collect();
        let mut gv: std::collections::HashMap<String, Vec<f64>> = Default::default();
        for r in &runs {
            if let Some(b) = base.get(&std_key(r)) {
                let per_km = (r.time_sec.unwrap() - b) / r.distance as f64 * 1000.0;
                gv.entry(format!("{}|{}", r.surface, r.track_cond)).or_default().push(per_km);
            }
        }
        let going = gv
            .into_iter()
            .filter(|(_, v)| v.len() >= 20)
            .map(|(k, mut v)| (k, median(&mut v)))
            .collect();
        // 騎手の成績。同じ馬・同じ日の走りは一度だけ数える
        let mut seen = std::collections::HashSet::new();
        let mut jockey: std::collections::HashMap<String, (u32, u32)> = Default::default();
        for r in &runs {
            if r.jockey.is_empty() || !seen.insert((r.date.clone(), r.venue.clone(), r.race_name.clone(), r.horse_num)) {
                continue;
            }
            if let Some(f) = r.finish_rank {
                let e = jockey.entry(r.jockey.clone()).or_default();
                e.0 += 1;
                if f <= 3 {
                    e.1 += 1;
                }
            }
        }
        jockey.retain(|_, v| v.0 >= 5);
        StdTimes { base, going, jockey, sire: Default::default() }
    }

    /// 確定レースの結果を父馬の成績表に足す
    pub fn add_results(&mut self, info: &RaceInfo, entries: &[HorseEntry]) {
        for e in entries {
            if e.sire.is_empty() {
                continue;
            }
            if let Some(f) = e.actual_rank {
                let v = self.sire.entry(format!("{}|{}", e.sire, info.surface)).or_default();
                v.0 += 1;
                if f <= 3 {
                    v.1 += 1;
                }
            }
        }
    }

    pub fn sire_top3(&self, sire: &str, surface: &str) -> f64 {
        const PRIOR: f64 = 0.21;
        const K: f64 = 40.0;
        let (n, t) = self.sire.get(&format!("{}|{}", sire, surface)).copied().unwrap_or((0, 0));
        (t as f64 + PRIOR * K) / (n as f64 + K)
    }

    /// 騎手の3着内率（騎乗数が少ない騎手は全体平均 0.25 に寄せる）。出馬表の略称は前方一致で探す
    pub fn jockey_top3(&self, name: &str) -> f64 {
        const PRIOR: f64 = 0.25;
        const K: f64 = 30.0;
        if name.is_empty() {
            return PRIOR;
        }
        let (mut rides, mut top3) = (0u32, 0u32);
        for (full, (n, t)) in &self.jockey {
            if full.starts_with(name) || name.starts_with(full.as_str()) {
                rides += n;
                top3 += t;
            }
        }
        (top3 as f64 + PRIOR * K) / (rides as f64 + K)
    }

    pub fn from_tables(
        base: &[(&str, f64)],
        going: &[(&str, f64)],
        jockey: &[(&str, u32, u32)],
        sire: &[(&str, u32, u32)],
    ) -> Self {
        StdTimes {
            base: base.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            going: going.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            jockey: jockey.iter().map(|(k, n, t)| (k.to_string(), (*n, *t))).collect(),
            sire: sire.iter().map(|(k, n, t)| (k.to_string(), (*n, *t))).collect(),
        }
    }

    /// 1000m あたり基準より何秒速かったか（大きいほど速い）。馬場状態の分は差し引く。
    pub fn figure(&self, r: &crate::models::PastRun) -> Option<f64> {
        let t = r.time_sec?;
        let b = self.base.get(&std_key(r))?;
        let per_km = (b - t) / r.distance as f64 * 1000.0;
        let g = self.going.get(&format!("{}|{}", r.surface, r.track_cond)).copied().unwrap_or(0.0);
        Some((per_km + g).clamp(-6.0, 6.0))
    }

    /// std_times.rs の中身として書き出す
    pub fn to_rust_source(&self) -> String {
        let mut b: Vec<_> = self.base.iter().collect();
        b.sort_by(|x, y| x.0.cmp(y.0));
        let mut g: Vec<_> = self.going.iter().collect();
        g.sort_by(|x, y| x.0.cmp(y.0));
        let mut s = String::from("//! 基準タイム表（自動生成）。`cargo run --release --example backtest -- <races.jsonl...> --write-std src/std_times.rs` で作り直す。\npub const BASE: &[(&str, f64)] = &[\n");
        for (k, v) in b {
            s += &format!("    ({:?}, {:.2}),\n", k, v);
        }
        s += "];\npub const GOING: &[(&str, f64)] = &[\n";
        for (k, v) in g {
            s += &format!("    ({:?}, {:.3}),\n", k, v);
        }
        s += "];\npub const JOCKEY: &[(&str, u32, u32)] = &[\n";
        let mut j: Vec<_> = self.jockey.iter().collect();
        j.sort_by(|x, y| x.0.cmp(y.0));
        for (k, (n, t)) in j {
            s += &format!("    ({:?}, {}, {}),\n", k, n, t);
        }
        s += "];\npub const SIRE: &[(&str, u32, u32)] = &[\n";
        let mut sr: Vec<_> = self.sire.iter().filter(|(_, v)| v.0 >= 5).collect();
        sr.sort_by(|x, y| x.0.cmp(y.0));
        for (k, (n, t)) in sr {
            s += &format!("    ({:?}, {}, {}),\n", k, n, t);
        }
        s += "];\n";
        s
    }
}

/// アプリに同梱した基準タイム表
pub static STD_TIMES: std::sync::LazyLock<StdTimes> =
    std::sync::LazyLock::new(|| StdTimes::from_tables(crate::std_times::BASE, crate::std_times::GOING, crate::std_times::JOCKEY, crate::std_times::SIRE));

fn parse_ymd(s: &str) -> Option<chrono::NaiveDate> {
    let n: Vec<i32> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect();
    if n.len() >= 3 {
        chrono::NaiveDate::from_ymd_opt(n[0], n[1] as u32, n[2] as u32)
    } else {
        None
    }
}

/// 時計・ローテーションの特徴量: [指数(最高), 指数(平均), 間隔(log日数), 距離変更, 芝ダ替わり]
pub fn form_features(e: &HorseEntry, info: &RaceInfo, std: &StdTimes) -> [f64; 5] {
    let figs: Vec<f64> = e.past_runs.iter().take(3).filter_map(|r| std.figure(r)).collect();
    let (best, avg) = if figs.is_empty() {
        (-1.0, -1.0) // 時計のない馬は控えめに置く
    } else {
        (
            figs.iter().cloned().fold(f64::MIN, f64::max),
            figs.iter().sum::<f64>() / figs.len() as f64,
        )
    };
    let last = e.past_runs.first();
    let rest = match (last.and_then(|r| parse_ymd(&r.date)), parse_ymd(&info.date_str)) {
        (Some(a), Some(b)) if b > a => ((b - a).num_days() as f64).ln(),
        _ => (60.0f64).ln(),
    };
    let dist_change = last.map(|r| ((info.distance as f64 - r.distance as f64).abs() / 400.0).min(3.0)).unwrap_or(0.0);
    let surf_change = last.map(|r| if r.surface != info.surface { 1.0 } else { 0.0 }).unwrap_or(0.0);
    [best, avg, rest, dist_change, surf_change]
}

/// 1頭分の特徴量（標準化前）
pub fn feature_row(p: &HorsePrediction, info: &RaceInfo, field_carried_weight: f64, std: &StdTimes) -> Vec<f64> {
    let mut v = vec![p.ability_score, p.speed_score, p.aptitude_score, p.jockey_frame_score, p.pace_score];
    v.extend(past_features(&p.entry, field_carried_weight));
    v.extend(form_features(&p.entry, info, std));
    let j = std.jockey_top3(&p.entry.jockey_name);
    v.push((j / (1.0 - j)).ln());
    let sr = std.sire_top3(&p.entry.sire, &info.surface);
    v.push((sr / (1.0 - sr)).ln());
    v
}

/// 列ごとにレース内で平均 0・標準偏差 1 にそろえる
pub fn standardize(rows: &[Vec<f64>]) -> Vec<Vec<f64>> {
    if rows.is_empty() {
        return Vec::new();
    }
    let n = rows.len() as f64;
    let k = rows[0].len();
    let mut out = rows.to_vec();
    for j in 0..k {
        let m = rows.iter().map(|r| r[j]).sum::<f64>() / n;
        let sd = (rows.iter().map(|r| (r[j] - m).powi(2)).sum::<f64>() / n).sqrt();
        for (i, r) in rows.iter().enumerate() {
            out[i][j] = if sd > 1e-9 { (r[j] - m) / sd } else { 0.0 };
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn run_prediction(
    race_info: RaceInfo,
    entries: Vec<HorseEntry>,
    official_payouts: Vec<OfficialPayoutItem>,
    weights: ScoringWeights,
    budget: u32,
    strategy: String,
    memo: String,
    combo_odds: ComboOdds,
) -> RacePredictionResult {
    run_prediction_with(race_info, entries, official_payouts, weights, budget, strategy, memo, combo_odds, None)
}

/// `jv_probs`（馬番 → 勝率）を渡すと、勝率と順位・印をそれに置き換える（JRA-VAN モデル）。
/// 5項目スコアは表示用にそのまま計算する。
#[allow(clippy::too_many_arguments)]
pub fn run_prediction_with(
    race_info: RaceInfo,
    entries: Vec<HorseEntry>,
    official_payouts: Vec<OfficialPayoutItem>,
    weights: ScoringWeights,
    budget: u32,
    strategy: String,
    memo: String,
    combo_odds: ComboOdds,
    jv: Option<&crate::models::JvModelOut>,
) -> RacePredictionResult {
    let jv_probs = jv.map(|j| &j.probs);
    let pace_analysis = analyze_pace(&race_info, &entries);

    // Collect field-wide average speed & last 3F for relative normalization
    let mut field_3f_values: Vec<f64> = Vec::new();
    for e in &entries {
        if e.is_scratched {
            continue;
        }
        for r in &e.past_runs {
            if let Some(l3f) = r.last_3f {
                if (31.0..42.0).contains(&l3f) {
                    field_3f_values.push(l3f);
                }
            }
        }
    }
    let avg_field_3f = if field_3f_values.is_empty() {
        35.0
    } else {
        field_3f_values.iter().sum::<f64>() / (field_3f_values.len() as f64)
    };

    let total_w = (weights.ability
        + weights.speed
        + weights.aptitude
        + weights.jockey_frame
        + weights.pace)
        .max(1.0);

    let mut preds: Vec<HorsePrediction> = Vec::new();

    for entry in entries {
        if entry.is_scratched {
            preds.push(HorsePrediction {
                entry,
                mark: "消".to_string(),
                rank_order: 99,
                total_index: 30.0,
                raw_score: 0.0,
                ability_score: 0.0,
                speed_score: 0.0,
                aptitude_score: 0.0,
                jockey_frame_score: 0.0,
                pace_score: 0.0,
                win_prob: 0.0,
                top3_prob: 0.0,
                expected_value: 0.0,
                value_rank: "C".to_string(),
                reasons: vec!["出走取消・除外".to_string()],
                training_score: None,
            });
            continue;
        }

        let mut reasons = Vec::new();

        // 1. 基礎能力・過去着順スコア (0..100)
        let ability_score = compute_ability_score(&entry, &mut reasons);

        // 2. スピード・上がり3F指数 (0..100)
        let speed_score = compute_speed_score(&entry, &race_info, avg_field_3f, &mut reasons);

        // 3. コース・距離・馬場適性スコア (0..100)
        let aptitude_score = compute_aptitude_score(&entry, &race_info, &mut reasons);

        // 4. 騎手・枠順・状態スコア (0..100)
        let jockey_frame_score = compute_jockey_frame_score(&entry, &race_info, &mut reasons);

        // 5. 展開・脚質バイアススコア (0..100)
        let pace_score = compute_pace_score(&entry, &pace_analysis, &mut reasons);

        let raw_score = (ability_score * weights.ability
            + speed_score * weights.speed
            + aptitude_score * weights.aptitude
            + jockey_frame_score * weights.jockey_frame
            + pace_score * weights.pace)
            / total_w;

        preds.push(HorsePrediction {
            entry,
            mark: String::new(),
            rank_order: 0,
            total_index: 50.0,
            raw_score,
            ability_score,
            speed_score,
            aptitude_score,
            jockey_frame_score,
            pace_score,
            win_prob: 0.0,
            top3_prob: 0.0,
            expected_value: 0.0,
            value_rank: "B".to_string(),
            reasons,
            training_score: None,
        });
    }

    // 勝率: 特徴量をレース内で標準化し、当てはめた係数で効用を出して softmax する。
    // 指数の重みはモデルの係数そのもの（手動の重みは使わない）。
    let active_idx: Vec<usize> = (0..preds.len())
        .filter(|&i| !preds[i].entry.is_scratched)
        .collect();
    let field_cw = if active_idx.is_empty() {
        0.0
    } else {
        active_idx.iter().map(|&i| preds[i].entry.carried_weight).sum::<f64>() / active_idx.len() as f64
    };
    let rows: Vec<Vec<f64>> = active_idx
        .iter()
        .map(|&i| feature_row(&preds[i], &race_info, field_cw, &STD_TIMES))
        .collect();
    let z = standardize(&rows);
    // 要素ごとの寄与（FACTOR_NAMES の順。基本モデルは 0..5 の5要素だけ）
    let mut contrib: Vec<[f64; 7]> = z
        .iter()
        .map(|row| {
            let mut c = [0.0; 7];
            for (f, x) in row.iter().enumerate() {
                c[FEATURE_GROUP[f]] += MODEL_BETA[f] * x;
            }
            c
        })
        .collect();
    let utility: Vec<f64> = contrib.iter().map(|c| c.iter().sum()).collect();
    if let Some(j) = jv {
        for (a, &i) in active_idx.iter().enumerate() {
            if let Some(v) = j.contrib.get(&preds[i].entry.umaban) {
                for (k, x) in v.iter().take(7).enumerate() {
                    contrib[a][k] = *x;
                }
            }
        }
    }
    let _ = &weights;
    // JRA-VAN モデルの勝率があれば、効用を log(勝率) に置き換える（順位・指数・印もそれに従う）
    let utility: Vec<f64> = match jv_probs {
        Some(m) => active_idx.iter().map(|&i| m.get(&preds[i].entry.umaban).copied().unwrap_or(1e-4).max(1e-6).ln()).collect(),
        None => utility,
    };
    for (a, &i) in active_idx.iter().enumerate() {
        preds[i].raw_score = utility[a];
    }
    let n = utility.len().max(1) as f64;
    let mean = utility.iter().sum::<f64>() / n;
    let std_dev = (utility.iter().map(|u| (u - mean).powi(2)).sum::<f64>() / n).sqrt().max(1e-6);
    let umax = utility.iter().cloned().fold(f64::MIN, f64::max);
    let win_probs = harville::normalize(&utility.iter().map(|u| (u - umax).exp()).collect::<Vec<_>>());
    let top3_probs = harville::Model::new(&win_probs).top3();

    // 表示用の5要素（＋調教）は、各要素の寄与をレース内の偏差値にしたもの。重みはモデルが決める
    let n_act = active_idx.len().max(1) as f64;
    let mut sds = [0.0f64; 7];
    let mut means = [0.0f64; 7];
    for k in 0..7 {
        means[k] = contrib.iter().map(|c| c[k]).sum::<f64>() / n_act;
        sds[k] = (contrib.iter().map(|c| (c[k] - means[k]).powi(2)).sum::<f64>() / n_act).sqrt();
    }
    let dev = |a: usize, k: usize| -> f64 {
        if sds[k] < 1e-9 { 50.0 } else { (((50.0 + (contrib[a][k] - means[k]) / sds[k] * 10.0) * 10.0).round() / 10.0).clamp(0.0, 100.0) }
    };
    for (a, &i) in active_idx.iter().enumerate() {
        let p = &mut preds[i];
        p.ability_score = dev(a, 0);
        p.speed_score = dev(a, 1);
        p.aptitude_score = dev(a, 2);
        p.jockey_frame_score = dev(a, 3);
        p.pace_score = dev(a, 4);
        p.training_score = if jv.is_some() { Some(dev(a, 5)) } else { None };
    }
    let sd_total: f64 = sds.iter().sum();
    let factor_shares: Vec<crate::models::FactorShare> = if sd_total < 1e-9 {
        Vec::new()
    } else {
        crate::models::FACTOR_NAMES
            .iter()
            .zip(sds.iter())
            .filter(|(_, sd)| **sd > 1e-9)
            .map(|(name, sd)| crate::models::FactorShare { name: name.to_string(), share: ((sd / sd_total) * 1000.0).round() / 1000.0 })
            .collect()
    };

    for (a, &i) in active_idx.iter().enumerate() {
        let p = &mut preds[i];
        let zs = (p.raw_score - mean) / std_dev;
        p.total_index = ((50.0 + zs * 10.0) * 10.0).round() / 10.0;
        p.win_prob = (win_probs[a] * 1000.0).round() / 1000.0;
        p.top3_prob = (top3_probs[a].min(1.0) * 1000.0).round() / 1000.0;

        let est_odds = p.entry.win_odds.unwrap_or_else(|| {
            // 発売前は自分の勝率から払戻率込みの公正オッズを置く（EV は払戻率と同じ値になる）
            (TAKEOUT_WIN / win_probs[a].max(0.002)).clamp(1.0, 999.9)
        });
        p.expected_value = ((win_probs[a] * est_odds) * 100.0).round() / 100.0;

        p.value_rank = if p.expected_value >= 1.35 && p.total_index >= 51.0 {
            "S".to_string()
        } else if p.expected_value >= 1.05 && p.total_index >= 48.0 {
            "A".to_string()
        } else if p.expected_value >= 0.80 {
            "B".to_string()
        } else {
            "C".to_string()
        };

        if p.expected_value >= 1.30 && p.total_index >= 50.0 {
            p.reasons.push(format!("期待値 {:.2} (妙味大)", p.expected_value));
        }
    }

    // Sort active horses by raw_score descending to assign rank_order and marks
    let mut indices: Vec<usize> = (0..preds.len())
        .filter(|&i| !preds[i].entry.is_scratched)
        .collect();
    indices.sort_by(|&a, &b| {
        preds[b]
            .raw_score
            .partial_cmp(&preds[a].raw_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Find best dark horse (☆) among ranks 4..=8 with highest expected_value
    let mut star_idx: Option<usize> = None;
    let mut best_ev = 1.05;
    for &idx in indices.iter().skip(3).take(6) {
        if preds[idx].expected_value > best_ev {
            best_ev = preds[idx].expected_value;
            star_idx = Some(idx);
        }
    }
    if star_idx.is_none() && indices.len() > 3 {
        star_idx = Some(indices[3]);
    }

    for (rank_zero, &idx) in indices.iter().enumerate() {
        let rank = (rank_zero + 1) as u32;
        preds[idx].rank_order = rank;
        preds[idx].mark = if rank == 1 {
            "◎".to_string()
        } else if rank == 2 {
            "○".to_string()
        } else if rank == 3 {
            "▲".to_string()
        } else if Some(idx) == star_idx {
            "☆".to_string()
        } else if rank <= 6 {
            "△".to_string()
        } else {
            String::new()
        };
    }

    // Sort predictions by rank_order for convenient display (UI can also sort by umaban)
    preds.sort_by_key(|p| (p.rank_order, p.entry.umaban));

    // Generate bet recommendations & verify against official payouts if finished
    let mut recommendations = generate_recommendations(
        &preds,
        budget,
        &strategy,
        &official_payouts,
        race_info.is_finished,
        &combo_odds,
        jv.is_some(),
    );

    let total_bet: u32 = recommendations.iter().map(|r| r.suggested_amount).sum();
    let total_return = if race_info.is_finished {
        Some(
            recommendations
                .iter()
                .map(|r| r.actual_return.unwrap_or(0))
                .sum(),
        )
    } else {
        None
    };

    let race_info_is_finished = race_info.is_finished;
    let roi_percent = total_return.map(|ret| {
        if total_bet > 0 {
            ((ret as f64 / total_bet as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        }
    });

    // Keep recommendations clean
    recommendations.retain(|r| r.suggested_amount > 0);

    RacePredictionResult {
        race_info,
        weights,
        budget,
        strategy,
        pace_analysis,
        predictions: preds,
        recommendations,
        official_payouts,
        total_bet,
        total_return,
        roi_percent,
        memo,
        saved_at: None,
        pre_race: !race_info_is_finished,
        model_source: if jv_probs.is_some() { "jravan".into() } else { "basic".into() },
        jv_probs: jv_probs.map(|m| {
            let mut v: Vec<(u32, f64)> = m.iter().map(|(k, p)| (*k, *p)).collect();
            v.sort_by_key(|x| x.0);
            v
        }),
        jv_contrib: jv.map(|j| {
            let mut v: Vec<(u32, Vec<f64>)> = j.contrib.iter().map(|(k, c)| (*k, c.clone())).collect();
            v.sort_by_key(|x| x.0);
            v
        }),
        factor_shares,
        combo_odds,
        my_bets: Vec::new(),
    }
}

/// 保存済みの予想に確定結果を当てはめる。買い目は作り直さず、保存時のものをそのまま採点する。
pub fn apply_official_result(
    mut result: RacePredictionResult,
    rows: &[ResultRow],
    payouts: Vec<OfficialPayoutItem>,
) -> RacePredictionResult {
    for row in rows {
        if let Some(p) = result
            .predictions
            .iter_mut()
            .find(|p| p.entry.umaban == row.umaban)
        {
            p.entry.actual_rank = Some(row.rank);
            if row.time.is_some() {
                p.entry.actual_time = row.time.clone();
            }
            if row.last_3f.is_some() {
                p.entry.actual_last_3f = row.last_3f;
            }
        }
    }
    result.race_info.is_finished = true;
    verify_recommendations_with_payouts(&mut result.recommendations, &payouts);
    verify_recommendations_with_payouts(&mut result.my_bets, &payouts);
    result.official_payouts = payouts;
    let total_return: u32 = result
        .recommendations
        .iter()
        .map(|r| r.actual_return.unwrap_or(0))
        .sum();
    result.total_bet = result.recommendations.iter().map(|r| r.suggested_amount).sum();
    result.total_return = Some(total_return);
    result.roi_percent = Some(if result.total_bet > 0 {
        ((total_return as f64 / result.total_bet as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    });
    result
}

fn compute_ability_score(entry: &HorseEntry, reasons: &mut Vec<String>) -> f64 {
    if entry.past_runs.is_empty() {
        return 50.0;
    }

    let recency_weights = [1.0, 0.75, 0.55, 0.40, 0.30];
    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;
    let mut g1_top3 = false;
    let mut recent_win = false;

    for (i, run) in entry.past_runs.iter().take(5).enumerate() {
        let w = recency_weights.get(i).copied().unwrap_or(0.25);
        let rank = run.finish_rank.unwrap_or(10);
        let heads = run.head_count.unwrap_or(16).max(5);

        // Base placement score (1st = 92, last = 25)
        let placement_ratio = 1.0 - ((rank.saturating_sub(1)) as f64 / (heads - 1) as f64).min(1.0);
        let mut run_score = 28.0 + placement_ratio * 62.0;

        // Margin adjustment (着差)
        if let Some(margin) = run.margin {
            if margin <= -0.3 {
                run_score += 8.0; // Won by 2+ lengths
            } else if margin < 0.0 {
                run_score += 4.0;
            } else if margin <= 0.2 && rank > 1 {
                run_score += 6.0; // Very close finish (0.1-0.2s behind winner)
            } else if margin >= 1.5 {
                run_score -= 8.0;
            }
        }

        // Grade bonus
        let grade_bonus = match run.grade.as_deref() {
            Some("GI") | Some("J・GI") => {
                if rank <= 3 {
                    g1_top3 = true;
                }
                14.0
            }
            Some("GII") | Some("J・GII") => 9.0,
            Some("GIII") | Some("J・GIII") => 6.0,
            Some("L") | Some("OP") => 3.0,
            _ => 0.0,
        };
        run_score += grade_bonus;

        // Outperformed popularity bonus
        if let Some(pop) = run.popularity {
            if rank <= 3 && pop >= 6 {
                run_score += 4.0;
            }
        }

        if i == 0 && rank == 1 {
            recent_win = true;
        }

        weighted_sum += run_score.clamp(15.0, 112.0) * w;
        weight_total += w;
    }

    let score = weighted_sum / weight_total.max(1.0);

    // 単勝オッズによる加点はしない（市場の評価をモデルに混ぜると、期待値の計算で二重に数えるため）

    let score = score.clamp(20.0, 99.0);
    if g1_top3 {
        reasons.push("GI好走実績".to_string());
    } else if recent_win {
        reasons.push("前走勝利・勢い◎".to_string());
    } else if score >= 75.0 {
        reasons.push("安定上位クラス".to_string());
    }
    (score * 10.0).round() / 10.0
}

fn compute_speed_score(
    entry: &HorseEntry,
    race_info: &RaceInfo,
    avg_field_3f: f64,
    reasons: &mut Vec<String>,
) -> f64 {
    if entry.past_runs.is_empty() {
        return 50.0;
    }

    let mut best_3f_diff = -99.0_f64;
    let mut speed_ratings = Vec::new();

    for (i, run) in entry.past_runs.iter().take(5).enumerate() {
        let mut s = 55.0;

        // Evaluate last 3F relative to field average
        if let Some(l3f) = run.last_3f {
            let diff = avg_field_3f - l3f; // positive means faster than average
            if diff > best_3f_diff {
                best_3f_diff = diff;
            }
            s += (diff * 12.0).clamp(-20.0, 28.0);
        }

        // Standard time speed evaluation (m/s) adjusted for distance
        if let Some(t_sec) = run.time_sec {
            if t_sec > 30.0 && run.distance >= 1000 {
                let m_per_sec = (run.distance as f64) / t_sec;
                // Expected m/s baseline
                let baseline_mps = if run.surface == "芝" {
                    16.95 - ((run.distance as f64 - 1600.0) / 1000.0) * 0.35
                } else {
                    16.05 - ((run.distance as f64 - 1600.0) / 1000.0) * 0.32
                };
                let mps_diff = m_per_sec - baseline_mps;
                s += (mps_diff * 25.0).clamp(-15.0, 20.0);
            }
        }

        let weight = if i == 0 { 1.0 } else { 0.7 };
        speed_ratings.push((s.clamp(20.0, 98.0), weight));
    }

    let total_w: f64 = speed_ratings.iter().map(|(_, w)| w).sum();
    let avg_s = speed_ratings.iter().map(|(v, w)| v * w).sum::<f64>() / total_w.max(1.0);

    // Bonus if best 3F is very sharp
    let final_s = if best_3f_diff >= 1.0 {
        reasons.push("鬼脚・上がり最速級".to_string());
        (avg_s + 6.0).clamp(25.0, 98.0)
    } else if best_3f_diff >= 0.5 && race_info.surface == "芝" {
        reasons.push("鋭い末脚".to_string());
        (avg_s + 3.0).clamp(25.0, 96.0)
    } else {
        avg_s.clamp(25.0, 95.0)
    };

    (final_s * 10.0).round() / 10.0
}

fn compute_aptitude_score(
    entry: &HorseEntry,
    race_info: &RaceInfo,
    reasons: &mut Vec<String>,
) -> f64 {
    if entry.past_runs.is_empty() {
        return 55.0;
    }

    let mut score = 55.0_f64;
    let mut same_dist_good = 0;
    let mut same_venue_good = 0;
    let mut same_surface_good = 0;
    let mut heavy_track_good = 0;

    for run in &entry.past_runs {
        let rank = run.finish_rank.unwrap_or(99);
        let is_good = rank <= 3;

        if run.surface == race_info.surface && is_good {
            same_surface_good += 1;
        }
        if run.surface == race_info.surface
            && (run.distance as i32 - race_info.distance as i32).abs() <= 200
            && is_good
        {
            same_dist_good += 1;
        }
        if !race_info.venue.is_empty() && run.venue.contains(&race_info.venue) && is_good {
            same_venue_good += 1;
        }
        if (race_info.track_cond == "稍"
            || race_info.track_cond == "重"
            || race_info.track_cond == "不")
            && (run.track_cond == "稍" || run.track_cond == "重" || run.track_cond == "不")
            && is_good
        {
            heavy_track_good += 1;
        }
    }

    score += (same_surface_good.min(3) as f64) * 4.5;
    score += (same_dist_good.min(3) as f64) * 7.5;
    score += (same_venue_good.min(2) as f64) * 6.5;
    score += (heavy_track_good.min(2) as f64) * 6.0;

    if same_venue_good >= 1 && same_dist_good >= 1 {
        reasons.push(format!(
            "{}{}{}m適性◎",
            race_info.venue, race_info.surface, race_info.distance
        ));
    } else if same_dist_good >= 2 {
        reasons.push(format!("{}m距離巧者", race_info.distance));
    }
    if heavy_track_good >= 1 && race_info.track_cond != "良" {
        reasons.push("道悪・重馬場巧者".to_string());
    }

    (score.clamp(30.0, 98.0) * 10.0).round() / 10.0
}

fn compute_jockey_frame_score(
    entry: &HorseEntry,
    race_info: &RaceInfo,
    reasons: &mut Vec<String>,
) -> f64 {
    let mut score = 55.0_f64;

    // Leading jockeys tier list (JRA + NAR地方競馬トップ騎手)
    let tier1_jockeys = [
        "ルメー", "川田", "モレイ", "C.デ", "レーン", "マーフ", "吉原寛", "矢野貴", "笹川翼",
        "御神本", "吉村智", "赤岡修", "山口勲", "石川倭", "落合玄",
    ];
    let tier2_jockeys = [
        "戸崎", "武豊", "坂井", "松山", "横山武", "横山典", "鮫島駿", "西村淳", "三浦",
        "池添", "岩田望", "藤岡佑", "菅原明", "佐々木", "桑村真", "服部茂", "阿部龍",
        "和田譲", "本橋孝", "森泰斗", "町田直", "張田昂", "下原理", "広瀬航", "宮川実",
        "飛田愛", "岡部誠", "丸野勝", "渡邊竜", "村上忍", "山本聡", "高松亮", "青柳正",
    ];

    if tier1_jockeys.iter().any(|&j| entry.jockey_name.contains(j)) {
        score += 22.0;
        reasons.push(format!("トップ騎手({})", entry.jockey_name));
    } else if tier2_jockeys.iter().any(|&j| entry.jockey_name.contains(j)) {
        score += 12.0;
    }

    // Continuous ride bonus
    if let Some(last_run) = entry.past_runs.first() {
        if !entry.jockey_name.is_empty() && last_run.jockey.contains(&entry.jockey_name) {
            score += 4.0;
        }
    }

    // Frame (Waku) bias
    if race_info.surface == "芝" && race_info.distance >= 1800 {
        if entry.waku <= 3 {
            score += 7.0;
            if entry.waku == 1 {
                reasons.push("絶好の内枠".to_string());
            }
        } else if entry.waku >= 7 {
            score -= 4.0;
        }
    } else if race_info.surface == "ダ" && race_info.distance <= 1400 {
        // Dirt sprints often favor outer gates
        if entry.waku >= 6 {
            score += 5.0;
        }
    }

    // Body weight change check
    if let Some(diff) = entry.weight_diff {
        if diff.abs() >= 14 {
            score -= 8.0;
        } else if diff.abs() <= 4 {
            score += 3.0;
        }
    }

    (score.clamp(25.0, 96.0) * 10.0).round() / 10.0
}

pub fn analyze_pace(race_info: &RaceInfo, entries: &[HorseEntry]) -> PaceAnalysis {
    let active: Vec<&HorseEntry> = entries.iter().filter(|e| !e.is_scratched).collect();
    let total = active.len().max(1);

    let mut front_runners = Vec::new();
    let mut stalkers_count = 0;

    for e in &active {
        if e.running_style.contains('逃') {
            front_runners.push(format!("{}番{}", e.umaban, e.horse_name));
        } else if e.running_style.contains('先') {
            stalkers_count += 1;
        }
    }

    let front_ratio = (front_runners.len() + stalkers_count) as f64 / (total as f64);

    let (predicted_pace, favored_style, commentary) =
        if front_runners.len() >= 3 || front_ratio >= 0.50 {
            (
                "ハイペース".to_string(),
                "差し・追込有利".to_string(),
                format!(
                    "逃げ・先行勢が揃い（先行比率 {:.0}%）、前半から淀みないハイペースが予想されます。{}コース{}mの直線で末脚を伸ばせる「差し・追込」勢に展開の利があります。",
                    front_ratio * 100.0,
                    race_info.venue,
                    race_info.distance
                ),
            )
        } else if front_runners.len() <= 1 && front_ratio <= 0.30 {
            (
                "スローペース".to_string(),
                "逃げ・先行有利".to_string(),
                format!(
                    "明確な逃げ馬が少なく（先行比率 {:.0}%）、道中は落ち着いたスローペースからの瞬発力勝負が見込まれます。前目で立ち回れる「逃げ・先行」勢の粘り込みに要注意です。",
                    front_ratio * 100.0
                ),
            )
        } else {
            (
                "平均ペース".to_string(),
                "好位・好脚質フラット".to_string(),
                format!(
                    "先行勢と差し勢のバランスが取れたメンバー構成（先行比率 {:.0}%）で、淀みのない平均ペースが予想されます。地力とコース適性が素直に問われる一戦です。",
                    front_ratio * 100.0
                ),
            )
        };

    PaceAnalysis {
        predicted_pace,
        front_runners,
        favored_style,
        commentary,
    }
}

fn compute_pace_score(
    entry: &HorseEntry,
    pace: &PaceAnalysis,
    reasons: &mut Vec<String>,
) -> f64 {
    let mut score = 58.0_f64;
    let style = entry.running_style.as_str();

    match pace.predicted_pace.as_str() {
        "ハイペース" => {
            if style.contains('差') || style.contains('追') {
                score += 18.0;
                reasons.push("ハイペース差し向き".to_string());
            } else if style.contains('逃') {
                score -= 10.0;
            }
        }
        "スローペース" => {
            if style.contains('逃') || style.contains('先') {
                score += 18.0;
                reasons.push("スロー前残り恩恵".to_string());
            } else if style.contains('追') {
                score -= 10.0;
            }
        }
        _ => {
            if style.contains('先') || style.contains('差') {
                score += 10.0;
            }
        }
    }

    (score.clamp(30.0, 95.0) * 10.0).round() / 10.0
}

pub fn generate_recommendations(
    preds: &[HorsePrediction],
    budget: u32,
    strategy: &str,
    official_payouts: &[OfficialPayoutItem],
    is_finished: bool,
    combo_odds: &ComboOdds,
    validated_model: bool,
) -> Vec<BetRecommendation> {
    let active: Vec<&HorsePrediction> = preds.iter().filter(|p| !p.entry.is_scratched).collect();
    // 買い目はこれまでどおり印から組む（期待値に関係なく買う）。
    // そのうえで、検証で残ったルール（JRA-VAN モデルで、実オッズでの期待値が 1.0 以上の単勝）に当たるものだけを「おすすめ」にする。
    // ほかの買い目は「おすすめしない」（検証では回収率 82〜84%）。docs/BACKTEST.md「買い方の検証結果」
    if active.len() < 3 {
        return Vec::new();
    }


    // モデル勝率（合計 1）と、単勝オッズから逆算した市場の勝率（全頭のオッズがあるときだけ）
    let model_p = harville::normalize(&active.iter().map(|p| p.win_prob).collect::<Vec<_>>());
    let market_p: Option<Vec<f64>> = active
        .iter()
        .map(|p| p.entry.win_odds.filter(|&o| o > 0.0).map(|o| 1.0 / o))
        .collect::<Option<Vec<_>>>()
        .map(|v| harville::normalize(&v));
    // 推定オッズは市場の勝率から作る。市場がなければモデル勝率から作る（このとき EV は払戻率に等しく、判断材料にならない）
    let odds_basis: &[f64] = market_p.as_deref().unwrap_or(&model_p);
    let hm = harville::Model::new(&model_p);
    let hb = harville::Model::new(odds_basis);

    let honmei = 0;
    let taikou = 1;
    let tanana = 2;
    let star = active
        .iter()
        .position(|p| p.mark == "☆")
        .unwrap_or(if active.len() > 3 { 3 } else { tanana });
    let renka1 = active.iter().position(|p| p.mark == "△").unwrap_or(star);

    let mut recs: Vec<BetRecommendation> = Vec::new();

    let push = |bet_type: &str, idx: &[usize], conf: &str, recs: &mut Vec<BetRecommendation>| {
        let mut uniq = idx.to_vec();
        uniq.sort_unstable();
        uniq.dedup();
        if uniq.len() != idx.len() {
            return;
        }
        let horses_unsorted: Vec<u32> = idx.iter().map(|&i| active[i].entry.umaban).collect();
        let mut horses = horses_unsorted.clone();
        if bet_type != "単勝" {
            horses.sort_unstable();
        }
        if recs.iter().any(|r| r.bet_type == bet_type && r.horses == horses) {
            return;
        }
        let key = combo_key(&horses);
        let (hit_prob, real_odds, basis_prob, takeout) = match bet_type {
            "単勝" => (
                model_p[idx[0]],
                active[idx[0]].entry.win_odds,
                odds_basis[idx[0]],
                TAKEOUT_WIN,
            ),
            "馬連" => (
                hm.quinella(idx[0], idx[1]),
                combo_odds.umaren.get(&key).copied(),
                hb.quinella(idx[0], idx[1]),
                TAKEOUT_UMAREN,
            ),
            "ワイド" => (
                hm.wide(idx[0], idx[1]),
                combo_odds.wide.get(&key).copied(),
                hb.wide(idx[0], idx[1]),
                TAKEOUT_WIDE,
            ),
            _ => (
                hm.trio(idx[0], idx[1], idx[2]),
                combo_odds.sanrenpuku.get(&key).copied(),
                hb.trio(idx[0], idx[1], idx[2]),
                TAKEOUT_TRIO,
            ),
        };
        let (odds, estimated) = match real_odds {
            Some(o) => (o, false),
            None => ((takeout / basis_prob.max(1e-6)).clamp(1.0, 99_999.0), true),
        };
        let odds = (odds * 10.0).round() / 10.0;
        let combination_text = if bet_type == "単勝" {
            format!("{} ({})", horses[0], active[idx[0]].mark)
        } else {
            horses
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(" - ")
        };
        recs.push(BetRecommendation {
            bet_type: bet_type.to_string(),
            horses,
            horse_names: idx.iter().map(|&i| active[i].entry.horse_name.clone()).collect(),
            combination_text,
            est_hit_prob: (hit_prob * 1000.0).round() / 1000.0,
            est_odds: odds,
            expected_value: ((hit_prob * odds) * 100.0).round() / 100.0,
            suggested_amount: 0,
            confidence: conf.to_string(),
            is_hit: None,
            actual_payout_per_100: None,
            actual_return: None,
            odds_is_estimated: estimated,
            recommended: false,
        });
    };

    // 1. 単勝 (◎ or ☆ depending on strategy)
    let tan_target = if strategy == "value_focus"
        && active[star].expected_value > active[honmei].expected_value
    {
        star
    } else {
        honmei
    };
    let tan_conf = if active[tan_target].mark == "◎" {
        "本命勝負"
    } else {
        "高期待値単勝"
    };
    push("単勝", &[tan_target], tan_conf, &mut recs);

    // 2. 馬連 (◎-○, ◎-▲, ◎-☆)
    push("馬連", &[honmei, taikou], "本線", &mut recs);
    push("馬連", &[honmei, tanana], "対抗押さえ", &mut recs);
    push("馬連", &[honmei, star], "妙味狙い", &mut recs);

    // 3. ワイド (◎-○, ◎-☆)
    push("ワイド", &[honmei, taikou], "鉄板ワイド", &mut recs);
    push("ワイド", &[honmei, star], "穴ワイド妙味", &mut recs);

    // 4. 3連複 (◎-○-▲, ◎-○-☆, ◎-▲-☆, ◎-○-△)
    push("3連複", &[honmei, taikou, tanana], "3連複本線", &mut recs);
    push("3連複", &[honmei, taikou, star], "3連複中穴", &mut recs);
    push("3連複", &[honmei, tanana, star], "3連複高配当", &mut recs);
    push("3連複", &[honmei, taikou, renka1], "3連複押さえ", &mut recs);

    // 検証で残ったルールに当たる単勝（期待値 1.0 以上・実オッズ）を足す。おすすめの印を付ける
    if validated_model {
        for i in 0..active.len() {
            if let Some(o) = active[i].entry.win_odds.filter(|&o| o > 0.0) {
                if model_p[i] * o >= 1.0 {
                    push("単勝", &[i], "期待値1.0以上", &mut recs);
                }
            }
        }
    }
    for r in recs.iter_mut() {
        r.recommended = validated_model && r.bet_type == "単勝" && !r.odds_is_estimated && r.expected_value >= 1.0;
    }

    // Allocate budget across tickets in 100-yen units according to strategy
    let clean_budget = (budget / 100) * 100;
    let scores: Vec<f64> = recs
        .iter()
        .map(|r| match strategy {
            "hit_focus" => r.est_hit_prob.powf(0.85) * 10.0,
            "value_focus" => r.expected_value.max(0.4).powf(1.4) * (r.est_hit_prob.sqrt()),
            _ => (r.est_hit_prob * 4.0) + (r.expected_value * 0.8),
        })
        .collect();
    let score_sum: f64 = scores.iter().sum::<f64>().max(1e-6);

    let mut allocated = 0_u32;
    for (i, r) in recs.iter_mut().enumerate() {
        let raw_share = (clean_budget as f64) * (scores[i] / score_sum);
        let rounded = ((raw_share / 100.0).round() as u32 * 100).max(100);
        if allocated + rounded <= clean_budget {
            r.suggested_amount = rounded;
            allocated += rounded;
        }
    }
    if allocated < clean_budget && !recs.is_empty() {
        recs[0].suggested_amount += clean_budget - allocated;
    }
    // おすすめの単勝は、予算の配分で 0 円になっても 100 円は買う（検証した買い方は 1 点 100 円）
    for r in recs.iter_mut().filter(|r| r.recommended && r.suggested_amount == 0) {
        r.suggested_amount = 100;
    }

    // Verify against official payouts if finished
    if is_finished {
        verify_recommendations_with_payouts(&mut recs, official_payouts);
    }

    recs
}

pub fn verify_recommendations_with_payouts(
    recs: &mut [BetRecommendation],
    official_payouts: &[OfficialPayoutItem],
) {
    for rec in recs.iter_mut() {
        let mut hit_payout: Option<u32> = None;

        for pay in official_payouts {
            if pay.bet_type != rec.bet_type {
                continue;
            }
            if rec.bet_type == "馬単" || rec.bet_type == "3連単" {
                if pay.combination == rec.horses {
                    hit_payout = Some(pay.payout_yen);
                    break;
                }
            } else {
                let mut p_sorted = pay.combination.clone();
                let mut r_sorted = rec.horses.clone();
                p_sorted.sort();
                r_sorted.sort();
                if p_sorted == r_sorted {
                    hit_payout = Some(pay.payout_yen);
                    break;
                }
            }
        }

        if let Some(p100) = hit_payout {
            rec.is_hit = Some(true);
            rec.actual_payout_per_100 = Some(p100);
            rec.actual_return = Some((rec.suggested_amount / 100) * p100);
        } else {
            rec.is_hit = Some(false);
            rec.actual_payout_per_100 = None;
            rec.actual_return = Some(0);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::{OfficialPayoutItem, PastRun, RaceInfo};

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    pub(crate) fn race_info(finished: bool) -> RaceInfo {
        RaceInfo {
            race_id: "202305050812".into(),
            race_name: "テスト".into(),
            race_num: "12R".into(),
            date_str: "2023年11月26日".into(),
            venue: "東京".into(),
            grade: None,
            start_time: "15:40".into(),
            surface: "芝".into(),
            distance: 2400,
            direction: "左".into(),
            weather: "晴".into(),
            track_cond: "良".into(),
            head_count: 6,
            is_finished: finished,
            source_url: String::new(),
        }
    }

    pub(crate) fn entry(umaban: u32, finish: u32, odds: f64) -> HorseEntry {
        HorseEntry {
            waku: umaban,
            umaban,
            horse_id: String::new(),
            horse_name: format!("馬{}", umaban),
            sex_age: "牡4".into(),
            carried_weight: 58.0,
            jockey_name: String::new(),
            jockey_id: String::new(),
            trainer_name: String::new(),
            sire: String::new(),
            dam: String::new(),
            broodmare_sire: String::new(),
            running_style: "先".into(),
            interval_text: String::new(),
            body_weight: None,
            weight_diff: None,
            win_odds: Some(odds),
            place_odds_min: None,
            place_odds_max: None,
            popularity: None,
            is_scratched: false,
            past_runs: vec![PastRun {
                date: "2023.10.01".into(),
                venue: "東京".into(),
                race_name: "前走".into(),
                grade: None,
                finish_rank: Some(finish),
                surface: "芝".into(),
                distance: 2400,
                time_str: String::new(),
                time_sec: None,
                track_cond: "良".into(),
                head_count: Some(12),
                horse_num: None,
                popularity: None,
                jockey: String::new(),
                carried_weight: None,
                passage: String::new(),
                last_3f: None,
                margin: None,
            }],
            actual_rank: None,
            actual_time: None,
            actual_last_3f: None,
        }
    }

    fn field() -> Vec<HorseEntry> {
        vec![
            entry(1, 1, 2.5),
            entry(2, 2, 4.0),
            entry(3, 4, 6.0),
            entry(4, 6, 10.0),
            entry(5, 9, 20.0),
            entry(6, 12, 40.0),
        ]
    }

    #[test]
    fn harville_probabilities_sum_to_place_counts() {
        let p = harville::normalize(&[0.4, 0.25, 0.15, 0.1, 0.06, 0.04]);
        let n = p.len();
        let h = harville::Model::new(&p);
        let mut quinella = 0.0;
        let mut wide = 0.0;
        let mut trio = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                quinella += h.quinella(i, j);
                wide += h.wide(i, j);
                for k in (j + 1)..n {
                    trio += h.trio(i, j, k);
                }
            }
        }
        assert!(approx(quinella, 1.0), "馬連の合計 {}", quinella);
        assert!(approx(wide, 3.0), "ワイドの合計 {}", wide);
        assert!(approx(trio, 1.0), "3連複の合計 {}", trio);
        assert!(approx(h.top3().iter().sum::<f64>(), 3.0));
    }

    #[test]
    fn win_and_top3_probs_are_consistent() {
        let r = run_prediction(
            race_info(false),
            field(),
            vec![],
            ScoringWeights::default(),
            3000,
            "balance".into(),
            String::new(),
            ComboOdds::default(),
        );
        let win: f64 = r.predictions.iter().map(|p| p.win_prob).sum();
        let top3: f64 = r.predictions.iter().map(|p| p.top3_prob).sum();
        assert!((win - 1.0).abs() < 0.01, "勝率の合計 {}", win);
        assert!((top3 - 3.0).abs() < 0.01, "3着内率の合計 {}", top3);
        assert!(r.pre_race);
        // 単勝は実オッズ、組み合わせは実オッズが無いので推定。簡易モデルなので「おすすめ」はない
        assert!(r
            .recommendations
            .iter()
            .all(|x| x.odds_is_estimated == (x.bet_type != "単勝")));
        assert!(r.recommendations.iter().all(|x| !x.recommended));
    }

    /// JRA-VAN モデルの勝率を渡したときの予想（勝率は馬番 1..6 の順）
    fn jv_prediction(finished: bool) -> RacePredictionResult {
        let probs = [0.30, 0.10, 0.10, 0.15, 0.20, 0.15];
        let jv = crate::models::JvModelOut {
            probs: probs.iter().enumerate().map(|(i, p)| (i as u32 + 1, *p)).collect(),
            contrib: std::collections::HashMap::new(),
        };
        run_prediction_with(
            race_info(finished),
            field(),
            vec![],
            ScoringWeights::default(),
            3000,
            "balance".into(),
            String::new(),
            ComboOdds::default(),
            Some(&jv),
        )
    }

    #[test]
    fn win_bets_with_ev_at_least_one_are_recommended() {
        // 期待値: 1番 0.30×2.5=0.75, 2番 0.40, 3番 0.60, 4番 1.50, 5番 4.0, 6番 6.0
        let r = jv_prediction(false);
        let mut rec: Vec<u32> = r.recommendations.iter().filter(|x| x.recommended).map(|x| x.horses[0]).collect();
        rec.sort_unstable();
        assert_eq!(rec, vec![4, 5, 6]);
        assert!(r.recommendations.iter().filter(|x| x.recommended).all(|x| x.bet_type == "単勝" && x.suggested_amount >= 100));
        // ほかの買い目（馬連など）も通常どおり出る
        assert!(r.recommendations.iter().any(|x| x.bet_type == "馬連" && x.suggested_amount > 0 && !x.recommended));
    }

    #[test]
    fn real_combo_odds_are_used_when_available() {
        let mut combo = ComboOdds::default();
        combo.umaren.insert("1-2".into(), 7.7);
        let r = run_prediction(race_info(false), field(), vec![], ScoringWeights::default(), 3000, "balance".into(), String::new(), combo);
        let umaren = r
            .recommendations
            .iter()
            .find(|x| x.bet_type == "馬連" && x.horses == vec![1, 2])
            .expect("◎-○ の馬連がある");
        assert_eq!(umaren.est_odds, 7.7);
        assert!(!umaren.odds_is_estimated);
    }

    #[test]
    fn finished_race_prediction_is_not_pre_race() {
        let r = run_prediction(
            race_info(true),
            field(),
            vec![],
            ScoringWeights::default(),
            3000,
            "balance".into(),
            String::new(),
            ComboOdds::default(),
        );
        assert!(!r.pre_race);
    }

    #[test]
    fn without_ev_one_tickets_are_still_bought_but_not_recommended() {
        // オッズを 0.8 倍にして控除率を作り、勝率は市場どおりにする → 全頭の期待値は 1.0 未満
        let mut f = field();
        for e in f.iter_mut() {
            e.win_odds = e.win_odds.map(|o| o * 0.8);
        }
        let inv: Vec<f64> = f.iter().map(|e| 1.0 / e.win_odds.unwrap()).collect();
        let s: f64 = inv.iter().sum();
        // 3番だけ少し高く見積もる（期待値が最大になる馬）
        let mut probs: Vec<f64> = inv.iter().map(|x| x / s).collect();
        probs[2] *= 1.1;
        let t: f64 = probs.iter().sum();
        let jv = crate::models::JvModelOut {
            probs: probs.iter().enumerate().map(|(i, p)| (i as u32 + 1, p / t)).collect(),
            contrib: std::collections::HashMap::new(),
        };
        let r = run_prediction_with(race_info(false), f, vec![], ScoringWeights::default(), 3000, "balance".into(), String::new(), ComboOdds::default(), Some(&jv));
        assert!(r.recommendations.iter().any(|x| x.suggested_amount > 0), "期待値に関係なく買い目は出る");
        assert!(r.recommendations.iter().all(|x| !x.recommended), "期待値 1.0 以上がないので、おすすめはない");
    }

    #[test]
    fn applying_result_grades_saved_bets_without_rebuilding_them() {
        let saved = jv_prediction(false);
        let before: Vec<(String, Vec<u32>, u32)> = saved
            .recommendations
            .iter()
            .map(|r| (r.bet_type.clone(), r.horses.clone(), r.suggested_amount))
            .collect();
        let tan = saved
            .recommendations
            .iter()
            .find(|r| r.bet_type == "単勝")
            .unwrap()
            .clone();
        let rows = vec![ResultRow {
            umaban: tan.horses[0],
            rank: 1,
            time: None,
            last_3f: None,
            win_odds: Some(99.0),
            popularity: None,
            corner_last: None,
        }];
        let payouts = vec![OfficialPayoutItem {
            bet_type: "単勝".into(),
            combination: tan.horses.clone(),
            combination_text: String::new(),
            payout_yen: 250,
            popularity: Some(1),
        }];
        let graded = apply_official_result(saved, &rows, payouts);
        let after: Vec<(String, Vec<u32>, u32)> = graded
            .recommendations
            .iter()
            .map(|r| (r.bet_type.clone(), r.horses.clone(), r.suggested_amount))
            .collect();
        assert_eq!(before, after, "買い目が組み直されている");
        assert!(graded.pre_race);
        assert!(graded.race_info.is_finished);
        assert_eq!(
            graded.total_return,
            Some(tan.suggested_amount / 100 * 250)
        );
        // 確定オッズで保存済みのオッズを上書きしない
        assert!(graded
            .predictions
            .iter()
            .all(|p| p.entry.win_odds != Some(99.0)));
    }
}
