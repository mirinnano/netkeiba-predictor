//! 集めた確定レースで予測の当たり具合を測り、勝率モデルの係数を当てはめる。
//!
//!   cargo run --release --example backtest -- <races.jsonl>... [--write-std <path>] [--l2 0.2]
//!
//! 評価は時系列ローリング: 日付順に 8 区間に分け、k 区間目までで学習して k+1 区間目で検証する（k=4..7）。
//! 基準タイム表も学習側の過去走だけで作る。検証区間の数字だけを判断に使う。
use netkeiba_predictor_lib::models::{ComboOdds, HorseEntry, HorseRun, OfficialPayoutItem, RaceInfo, ScoringWeights};
use netkeiba_predictor_lib::predictor::{
    feature_row, harville, run_prediction, standardize, StdTimes, FEATURE_NAMES, N_FEATURES,
};
use serde::Deserialize;
use std::io::BufRead;

#[derive(Deserialize)]
struct Race {
    race_info: RaceInfo,
    entries: Vec<HorseEntry>,
    payouts: Vec<OfficialPayoutItem>,
    #[serde(default)]
    combo_odds: ComboOdds,
}

/// アプリで使う特徴量（ローテーションの3つは検証で悪化、展開は係数が負になり効き目がないので外す）
const ADOPTED: [usize; 13] = [0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 15, 16];

struct Runner {
    feats: Vec<f64>,
    market: f64,
    /// 確定単勝オッズ（払戻の近似）
    odds: f64,
    rank: u32,
    /// 当日の馬場の偏り × この馬の位置: [内外の偏り×枠, 前後の偏り×脚質]
    bias: [f64; 2],
    /// 過去レースの中身（展開・位置取り・上がり）から作った特徴量。レース内で標準化済み
    pace: Vec<f64>,
}

/// 馬ごとの競走成績（db.netkeiba.com）。--horses で読み込む
static HORSES: std::sync::OnceLock<std::collections::HashMap<String, Vec<HorseRun>>> = std::sync::OnceLock::new();

/// 芝ダ・距離ごとの「前半3F−後半3F」の中央値。ペースの速い遅いはこの差で見る
static PACE_BASE: std::sync::OnceLock<std::collections::HashMap<String, f64>> = std::sync::OnceLock::new();

fn pace_key(r: &HorseRun) -> String {
    format!("{}|{}", r.surface, r.distance.unwrap_or(0))
}

pub const PACE_NAMES: [&str; 5] = ["展開不利", "4角から挽回", "上がり差", "上がり上位率", "不利で僅差"];

fn slash_date_key(s: &str) -> u32 {
    let p: Vec<u32> = s.split('/').filter_map(|x| x.trim().parse().ok()).collect();
    if p.len() == 3 { p[0] * 10000 + p[1] * 100 + p[2] } else { 0 }
}

/// 当該レースより前の走だけを使って、展開・位置取りの特徴量を作る。データがなければ None
fn pace_features(horse_id: &str, race_date: u32) -> Option<[f64; 5]> {
    let runs = HORSES.get()?.get(horse_id)?;
    let rw = [1.0, 0.8, 0.6, 0.45, 0.35];
    let mut acc = [0.0f64; 5];
    let mut w = 0.0;
    let mut used = 0;
    for r in runs.iter().filter(|r| {
        let d = slash_date_key(&r.date);
        d > 0 && d < race_date // 未来の走は絶対に使わない
    }) {
        if used >= 5 {
            break;
        }
        let (Some(heads), Some(rank)) = (r.head_count, r.finish_rank) else { continue };
        if heads < 5 {
            continue;
        }
        let Some(last_pos) = r.passage.split('-').filter_map(|x| x.parse::<f64>().ok()).last() else { continue };
        let pos = (last_pos / heads as f64).min(1.0); // 4角の位置（0=先頭,1=最後方）
        let fin = (rank as f64 - 1.0) / (heads as f64 - 1.0);
        let base = PACE_BASE.get().and_then(|m| m.get(&pace_key(r))).copied();
        let pace = match (r.race_first3f, r.race_last3f, base) {
            (Some(f), Some(l), Some(b)) => (f - l - b).clamp(-5.0, 5.0), // 正なら同条件の平均よりスロー
            _ => 0.0,
        };
        let against = pace * (pos - 0.5); // スローで後ろ、ハイで前ほど正 = 展開に逆らった
        let closing = match (r.race_last3f, r.last3f) {
            (Some(rl), Some(hl)) => (rl - hl).clamp(-3.0, 3.0),
            _ => 0.0,
        };
        let top3 = if r.last3f_rank.map(|k| k <= 3).unwrap_or(false) { 1.0 } else { 0.0 };
        let close_loss = against.max(0.0) * (-r.margin.unwrap_or(1.0).max(0.0)).exp();
        let k = rw[used];
        acc[0] += against * k;
        acc[1] += (pos - fin) * k;
        acc[2] += closing * k;
        acc[3] += top3 * k;
        acc[4] += close_loss * k;
        w += k;
        used += 1;
    }
    if used == 0 {
        return None;
    }
    Some(acc.map(|v| v / w))
}

/// 脚質を 0（逃げ）〜1（追込）の数に直す
fn style_score(s: &str) -> f64 {
    if s.contains('逃') {
        0.0
    } else if s.contains('先') {
        1.0 / 3.0
    } else if s.contains('差') {
        2.0 / 3.0
    } else if s.contains('追') {
        1.0
    } else {
        0.5
    }
}

/// 当日・同じ競馬場・同じ芝ダの、このレースより前のレース結果から馬場の偏りを測る。
/// inner: 3着以内馬の枠位置（0=内,1=外）の平均 − 出走馬全体の平均。負なら内有利。
/// front: 3着以内馬の脚質の平均 − 出走馬全体の平均。負なら前有利。
/// 前のレースが少ないほど 0 に寄せる。
fn day_bias(races: &[Race]) -> std::collections::HashMap<String, (f64, f64)> {
    let mut groups: std::collections::HashMap<(String, String, String), Vec<&Race>> = Default::default();
    for r in races {
        groups
            .entry((r.race_info.date_str.clone(), r.race_info.venue.clone(), r.race_info.surface.clone()))
            .or_default()
            .push(r);
    }
    let mut out = std::collections::HashMap::new();
    for (_, mut rs) in groups {
        rs.sort_by_key(|r| r.race_info.race_id.clone());
        let (mut si, mut sf, mut n) = (0.0, 0.0, 0.0);
        for r in rs {
            let shrink = n / (n + 2.0);
            let (bi, bf) = if n > 0.0 { (si / n * shrink, sf / n * shrink) } else { (0.0, 0.0) };
            out.insert(r.race_info.race_id.clone(), (bi, bf));
            let ent: Vec<_> = r.entries.iter().filter(|e| !e.is_scratched && e.actual_rank.is_some()).collect();
            let top: Vec<_> = ent.iter().filter(|e| e.actual_rank.unwrap() <= 3).collect();
            if ent.len() >= 5 && !top.is_empty() {
                let wk = |e: &&HorseEntry| (e.waku as f64 - 1.0) / 7.0;
                let all_w = ent.iter().map(wk).sum::<f64>() / ent.len() as f64;
                let top_w = top.iter().map(|e| wk(e)).sum::<f64>() / top.len() as f64;
                let all_s = ent.iter().map(|e| style_score(&e.running_style)).sum::<f64>() / ent.len() as f64;
                let top_s = top.iter().map(|e| style_score(&e.running_style)).sum::<f64>() / top.len() as f64;
                si += top_w - all_w;
                sf += top_s - all_s;
                n += 1.0;
            }
        }
    }
    out
}

fn date_key(s: &str) -> u32 {
    let n: Vec<u32> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect();
    if n.len() >= 3 {
        n[0] * 10000 + n[1] * 100 + n[2]
    } else {
        0
    }
}

fn load(paths: &[String]) -> Vec<Race> {
    let mut v: Vec<Race> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in paths {
        let f = std::fs::File::open(p).expect("データを開けません");
        for l in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            if let Ok(r) = serde_json::from_str::<Race>(&l) {
                if seen.insert(r.race_info.race_id.clone()) {
                    v.push(r);
                }
            }
        }
    }
    v.sort_by_key(|r| (date_key(&r.race_info.date_str), r.race_info.race_id.clone()));
    v
}

/// 予測を走らせ、アプリと同じ特徴量（全出走馬で標準化）を作る。評価には着順と単勝オッズのある馬だけ使う。
fn runners(r: &Race, std: &StdTimes, day: (f64, f64)) -> Option<Vec<Runner>> {
    let res = run_prediction(
        r.race_info.clone(),
        r.entries.clone(),
        r.payouts.clone(),
        ScoringWeights::default(),
        3000,
        "balance".into(),
        String::new(),
        r.combo_odds.clone(),
    );
    let active: Vec<_> = res.predictions.iter().filter(|p| !p.entry.is_scratched).collect();
    if active.len() < 5 {
        return None;
    }
    let cw = active.iter().map(|p| p.entry.carried_weight).sum::<f64>() / active.len() as f64;
    let rows: Vec<Vec<f64>> = active.iter().map(|p| feature_row(p, &r.race_info, cw, std)).collect();
    let z = standardize(&rows);
    let keep: Vec<usize> = (0..active.len())
        .filter(|&i| active[i].entry.win_odds.is_some())
        .collect();
    if keep.len() != active.len() || !keep.iter().any(|&i| active[i].entry.actual_rank == Some(1)) {
        return None;
    }
    let market = harville::normalize(&keep.iter().map(|&i| 1.0 / active[i].entry.win_odds.unwrap()).collect::<Vec<_>>());
    let rd = date_key(&r.race_info.date_str);
    let praw: Vec<Option<[f64; 5]>> = active.iter().map(|p| pace_features(&p.entry.horse_id, rd)).collect();
    // データのない馬はレース内の平均で埋めてから標準化する
    let mut prow: Vec<Vec<f64>> = Vec::new();
    for j in 0..5 {
        let vals: Vec<f64> = praw.iter().filter_map(|x| x.map(|a| a[j])).collect();
        let mean = if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 };
        for (i, x) in praw.iter().enumerate() {
            if j == 0 {
                prow.push(Vec::new());
            }
            prow[i].push(x.map(|a| a[j]).unwrap_or(mean));
        }
    }
    let pz = standardize(&prow);
    let wk = |i: usize| (active[i].entry.waku as f64 - 1.0) / 7.0;
    let st = |i: usize| style_score(&active[i].entry.running_style);
    let mw = (0..active.len()).map(wk).sum::<f64>() / active.len() as f64;
    let ms = (0..active.len()).map(st).sum::<f64>() / active.len() as f64;
    Some(
        keep.iter()
            .enumerate()
            .map(|(k, &i)| Runner {
                feats: z[i].clone(),
                market: market[k],
                odds: active[i].entry.win_odds.unwrap(),
                // 競走中止・失格で着順のない馬は負けとして数える
                rank: active[i].entry.actual_rank.unwrap_or(99),
                // 偏りと同じ向きの馬ほど正（内有利の日の内枠、前有利の日の先行馬）
                bias: [-day.0 * (wk(i) - mw) * 10.0, -day.1 * (st(i) - ms) * 10.0],
                pace: pz[i].clone(),
            })
            .collect(),
    )
}

fn softmax(v: &[f64]) -> Vec<f64> {
    let m = v.iter().cloned().fold(f64::MIN, f64::max);
    let e: Vec<f64> = v.iter().map(|x| (x - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.iter().map(|x| x / s).collect()
}

fn util(r: &Runner, beta: &[f64], mask: &[usize]) -> f64 {
    mask.iter().zip(beta).map(|(&f, b)| r.feats[f] * b).sum()
}

fn logloss(races: &[Vec<Runner>], prob: impl Fn(&[Runner]) -> Vec<f64>) -> f64 {
    let mut s = 0.0;
    for r in races {
        let p = prob(r);
        let w = r.iter().position(|x| x.rank == 1).unwrap();
        s -= p[w].max(1e-9).ln();
    }
    s / races.len() as f64
}

/// 条件付きロジット（1着の多項ロジット）を勾配法で当てはめる
fn fit(train: &[Vec<Runner>], mask: &[usize], l2: f64) -> Vec<f64> {
    let k = mask.len();
    let mut beta = vec![0.0; k];
    for _ in 0..2500 {
        let mut g = vec![0.0; k];
        for race in train {
            let p = softmax(&race.iter().map(|r| util(r, &beta, mask)).collect::<Vec<_>>());
            let w = race.iter().position(|r| r.rank == 1).unwrap();
            for j in 0..k {
                let f = mask[j];
                let ex: f64 = p.iter().zip(race).map(|(pi, r)| pi * r.feats[f]).sum();
                g[j] += race[w].feats[f] - ex;
            }
        }
        for j in 0..k {
            beta[j] += 0.5 * (g[j] / train.len() as f64 - l2 * beta[j]);
        }
    }
    beta
}

/// 列を返す関数で多項ロジットを当てはめる。先頭の列（市場）は正則化しない。初期値は市場=1
fn fit_cols(train: &[Vec<Runner>], cols: &dyn Fn(&Runner) -> Vec<f64>, l2: f64) -> Vec<f64> {
    let k = cols(&train[0][0]).len();
    let mut beta = vec![0.0; k];
    beta[0] = 1.0;
    let xs: Vec<Vec<Vec<f64>>> = train.iter().map(|r| r.iter().map(cols).collect()).collect();
    for _ in 0..3000 {
        let mut g = vec![0.0; k];
        for (race, x) in train.iter().zip(&xs) {
            let p = softmax(&x.iter().map(|xi| xi.iter().zip(&beta).map(|(a, b)| a * b).sum()).collect::<Vec<_>>());
            let w = race.iter().position(|r| r.rank == 1).unwrap();
            for j in 0..k {
                let ex: f64 = p.iter().zip(x).map(|(pi, xi)| pi * xi[j]).sum();
                g[j] += x[w][j] - ex;
            }
        }
        for j in 0..k {
            let reg = if j == 0 { 0.0 } else { l2 * beta[j] };
            beta[j] += 0.3 * (g[j] / train.len() as f64 - reg);
        }
    }
    beta
}

fn model_prob<'a>(beta: &'a [f64], mask: &'a [usize]) -> impl Fn(&[Runner]) -> Vec<f64> + 'a {
    move |r: &[Runner]| softmax(&r.iter().map(|x| util(x, beta, mask)).collect::<Vec<_>>())
}

fn top3_loss(races: &[Vec<Runner>], prob: &dyn Fn(&[Runner]) -> Vec<f64>, l2: f64, l3: f64) -> f64 {
    let (mut s, mut n) = (0.0, 0.0);
    for r in races {
        let pos = |k: u32| r.iter().position(|x| x.rank == k);
        let (Some(a), Some(b), Some(c)) = (pos(1), pos(2), pos(3)) else { continue };
        let h = harville::Model::with_lambda(&prob(r), l2, l3);
        s -= h.trifecta(a, b, c).max(1e-12).ln();
        n += 1.0;
    }
    s / n
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    let mut write_std: Option<String> = None;
    let mut l2 = 0.2;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--write-std" => {
                write_std = args.get(i + 1).cloned();
                i += 1;
            }
            "--horses" => {
                let path = args[i + 1].clone();
                let mut m = std::collections::HashMap::new();
                let f = std::fs::File::open(&path).expect("馬データを開けません");
                for l in std::io::BufReader::new(f).lines().map_while(Result::ok) {
                    #[derive(Deserialize)]
                    struct H {
                        horse_id: String,
                        runs: Vec<HorseRun>,
                    }
                    if let Ok(h) = serde_json::from_str::<H>(&l) {
                        m.insert(h.horse_id, h.runs);
                    }
                }
                // ペースの基準は 2023年7月より前の走だけで作る（検証区間は 2023年12月以降）
                let mut by: std::collections::HashMap<String, Vec<f64>> = Default::default();
                for runs in m.values() {
                    for r in runs {
                        if let (Some(f), Some(l)) = (r.race_first3f, r.race_last3f) {
                            if slash_date_key(&r.date) < 20230701 {
                                by.entry(pace_key(r)).or_default().push(f - l);
                            }
                        }
                    }
                }
                let base: std::collections::HashMap<String, f64> = by
                    .into_iter()
                    .filter(|(_, v)| v.len() >= 20)
                    .map(|(k, mut v)| {
                        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        (k, v[v.len() / 2])
                    })
                    .collect();
                eprintln!("馬データ {} 頭 / ペース基準 {} 条件", m.len(), base.len());
                PACE_BASE.set(base).ok();
                HORSES.set(m).ok();
                i += 1;
            }
            "--l2" => {
                l2 = args[i + 1].parse().unwrap();
                i += 1;
            }
            f => files.push(f.to_string()),
        }
        i += 1;
    }
    let races = load(&files);

    let mut leaks = 0;
    for r in &races {
        let d = date_key(&r.race_info.date_str);
        leaks += r.entries.iter().flat_map(|e| &e.past_runs).filter(|p| date_key(&p.date) >= d).count();
    }
    println!(
        "レース数 {}（{} 〜 {}）/ 過去走の日付が当日以降: {}",
        races.len(),
        races.first().map(|r| r.race_info.date_str.clone()).unwrap_or_default(),
        races.last().map(|r| r.race_info.date_str.clone()).unwrap_or_default(),
        leaks
    );

    if let Some(h) = HORSES.get() {
        let (mut ent, mut cov, mut future, mut same_race) = (0, 0, 0, 0);
        for r in &races {
            let d = date_key(&r.race_info.date_str);
            for e in r.entries.iter().filter(|e| !e.is_scratched) {
                ent += 1;
                if pace_features(&e.horse_id, d).is_some() {
                    cov += 1;
                }
                if let Some(runs) = h.get(&e.horse_id) {
                    // ページには当該レース以降の走も載っている。使う側で除いていることの確認用に数える
                    future += runs.iter().filter(|x| slash_date_key(&x.date) >= d).count();
                    same_race += runs.iter().filter(|x| x.race_id == r.race_info.race_id).count();
                }
            }
        }
        println!(
            "馬データの網羅: {}/{} 頭 ({:.1}%) / ページ上の当日以降の走 {}（うち当該レース {}）は特徴量に使っていない",
            cov, ent, cov as f64 / ent.max(1) as f64 * 100.0, future, same_race
        );
    }

    let masks: Vec<(&str, Vec<usize>)> = vec![
        ("5項目", (0..5).collect()),
        ("+過去走(現行)", (0..10).collect()),
        ("+指数", (0..12).collect()),
        ("+指数+ローテ", (0..15).collect()),
        ("+指数+騎手", vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 15]),
        ("+指数+騎手+父", vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 15, 16]),
    ];

    let bias = day_bias(&races);
    let mut resid_sum = [0.0f64; 8];
    let folds = 8;
    let per = races.len() / folds;
    println!("\n[時系列ローリング検証: 1着の対数損失（小さいほど良い）]");
    print!("  検証区間            市場   ");
    for (n, _) in &masks {
        print!("{:>14}", n);
    }
    println!("   最良プールa");
    let mut sums = vec![0.0; masks.len() + 1];
    let mut resid_lines: Vec<String> = Vec::new();
    const THRESH: [f64; 5] = [0.7, 0.8, 0.85, 0.9, 0.95];
    let mut roi_all = (0.0f64, 0.0f64);
    let mut roi_m = [(0.0f64, 0.0f64); 5];
    let mut roi_g = [(0.0f64, 0.0f64); 5];
    let mut t3 = [0.0f64; 2];
    let mut cnt = 0.0;
    for k in 4..folds {
        let train_r = &races[..k * per];
        let test_r = &races[k * per..((k + 1) * per).min(races.len())];
        // 基準タイムと騎手は学習側の過去走から。父の成績は日付順に、そのレースより前の結果だけで作る
        let mut std = StdTimes::fit(train_r.iter().flat_map(|r| r.entries.iter().flat_map(|e| e.past_runs.iter())));
        let mut tr: Vec<Vec<Runner>> = Vec::new();
        let mut te: Vec<Vec<Runner>> = Vec::new();
        for (j, r) in races[..k * per + test_r.len()].iter().enumerate() {
            if let Some(x) = runners(r, &std, *bias.get(&r.race_info.race_id).unwrap_or(&(0.0, 0.0))) {
                if j < k * per { tr.push(x) } else { te.push(x) }
            }
            std.add_results(&r.race_info, &r.entries);
        }
        let lm = logloss(&te, |r: &[Runner]| r.iter().map(|x| x.market).collect());
        sums[0] += lm;
        print!(
            "  {}〜 ({:>3}R)  {:.4}",
            test_r[0].race_info.date_str,
            te.len(),
            lm
        );
        let mut best_full = None;
        for (mi, (_, mask)) in masks.iter().enumerate() {
            let b = fit(&tr, mask, l2);
            let v = logloss(&te, model_prob(&b, mask));
            sums[mi + 1] += v;
            print!("{:>14.4}", v);
            if mi == masks.len() - 1 {
                best_full = Some(b);
            }
        }
        // 市場との対数線形プール（a は学習側で選ぶ）
        let bf = best_full.unwrap();
        let mask = &masks.last().unwrap().1;
        let pool = |a: f64, set: &[Vec<Runner>]| {
            logloss(set, |r: &[Runner]| {
                let mp = model_prob(&bf, mask)(r);
                softmax(&r.iter().zip(&mp).map(|(x, m)| (1.0 - a) * x.market.max(1e-9).ln() + a * m.max(1e-9).ln()).collect::<Vec<_>>())
            })
        };
        let best_a = (0..=10).map(|i| i as f64 * 0.05).min_by(|a, b| pool(*a, &tr).partial_cmp(&pool(*b, &tr)).unwrap()).unwrap();
        println!("   {:.2}", best_a);
        // 市場に対する上積み: u = γ·log(市場) + β·(追加の列)。β には L2、γ は自由
        let mdl = |x: &Runner| -> f64 { mask.iter().zip(&bf).map(|(&f, b)| x.feats[f] * b).sum() };
        let variants: [(&str, Box<dyn Fn(&Runner) -> Vec<f64>>); 8] = [
            ("市場のみ", Box::new(|x: &Runner| vec![x.market.max(1e-9).ln()])),
            ("市場(γ補正)", Box::new(|x: &Runner| vec![x.market.max(1e-9).ln()])),
            ("+モデル", Box::new(move |x: &Runner| vec![x.market.max(1e-9).ln(), mdl(x)])),
            ("+当日バイアス", Box::new(|x: &Runner| vec![x.market.max(1e-9).ln(), x.bias[0], x.bias[1]])),
            ("+モデル+バイアス", Box::new(move |x: &Runner| vec![x.market.max(1e-9).ln(), mdl(x), x.bias[0], x.bias[1]])),
            ("+特徴量個別", Box::new(|x: &Runner| {
                let mut v = vec![x.market.max(1e-9).ln()];
                v.extend(ADOPTED.iter().map(|&f| x.feats[f]));
                v
            })),
            ("+展開系", Box::new(|x: &Runner| {
                let mut v = vec![x.market.max(1e-9).ln()];
                v.extend(x.pace.iter().copied());
                v
            })),
            ("+モデル+展開系", Box::new(move |x: &Runner| {
                let mut v = vec![x.market.max(1e-9).ln(), mdl(x)];
                v.extend(x.pace.iter().copied());
                v
            })),
        ];
        let mut line = String::new();
        for (vi, (_, cols)) in variants.iter().enumerate() {
            let v = if vi == 0 {
                lm
            } else {
                let b = fit_cols(&tr, cols.as_ref(), l2 * 0.25);
                logloss(&te, |r: &[Runner]| softmax(&r.iter().map(|x| cols(x).iter().zip(&b).map(|(a, c)| a * c).sum()).collect::<Vec<_>>()))
            };
            resid_sum[vi] += v;
            line += &format!("{:>14.4}", v);
        }
        resid_lines.push(format!("  {}〜{}", test_r[0].race_info.date_str, line));

        // 回収率: 市場+モデル（学習側で当てはめ）で p×オッズ が閾値以上の馬の単勝を100円ずつ
        let cols = &variants[5].1;
        let bm = fit_cols(&tr, cols.as_ref(), l2 * 0.25);
        let bg = fit_cols(&tr, variants[1].1.as_ref(), l2 * 0.25);
        for race in &te {
            let p = softmax(&race.iter().map(|x| cols(x).iter().zip(&bm).map(|(a, c)| a * c).sum()).collect::<Vec<_>>());
            let pg = softmax(&race.iter().map(|x| x.market.max(1e-9).ln() * bg[0]).collect::<Vec<_>>());
            for (i, x) in race.iter().enumerate() {
                let won = if x.rank == 1 { x.odds * 100.0 } else { 0.0 };
                roi_all.0 += 100.0;
                roi_all.1 += won;
                for (ti, t) in THRESH.iter().enumerate() {
                    if p[i] * x.odds >= *t {
                        roi_m[ti].0 += 100.0;
                        roi_m[ti].1 += won;
                    }
                    if ti == 0 && (p[i] * x.odds >= 1.0 || pg[i] * x.odds >= 1.0) && std::env::var("DEBUG_PICKS").is_ok() {
                        let sum_inv: f64 = race.iter().map(|y| 1.0 / y.odds).sum();
                        eprintln!("pick odds={:.1} market={:.3} pm={:.3} pg={:.3} rank={} n={} overround={:.3}", x.odds, x.market, p[i], pg[i], x.rank, race.len(), sum_inv);
                    }
                    if pg[i] * x.odds >= *t {
                        roi_g[ti].0 += 100.0;
                        roi_g[ti].1 += won;
                    }
                }
            }
        }

        let mp: &dyn Fn(&[Runner]) -> Vec<f64> = &|r: &[Runner]| r.iter().map(|x| x.market).collect();
        t3[0] += top3_loss(&te, mp, 1.0, 1.0);
        t3[1] += top3_loss(&te, mp, harville::LAMBDA2, harville::LAMBDA3);
        cnt += 1.0;
    }
    print!("  平均                   {:.4}", sums[0] / cnt);
    for v in &sums[1..] {
        print!("{:>14.4}", v / cnt);
    }
    println!();
    println!("\n[市場に上積みできるか: 1着の対数損失（市場のみより小さければ、オッズにない情報がある）]");
    println!("  検証区間            {:>14}{:>14}{:>14}{:>14}{:>14}{:>14}{:>14}{:>14}", "市場のみ", "市場(γ補正)", "+モデル", "+当日バイアス", "+モデル+バイアス", "+特徴量個別", "+展開系", "+モデル+展開系");
    for l in &resid_lines {
        println!("{}", l);
    }
    print!("  平均                ");
    for v in resid_sum {
        print!("{:>14.4}", v / cnt);
    }
    println!();
    println!("\n[単勝の回収率（検証区間のみ, 確定オッズで近似）]");
    println!("  全馬を買う: {:.1}%（{} 点）", roi_all.1 / roi_all.0 * 100.0, (roi_all.0 / 100.0) as u64);
    for (ti, t) in THRESH.iter().enumerate() {
        println!(
            "  期待値≧{:.1}: 市場+モデル {:>5.1}%（{:>4} 点）  市場(γ補正)のみ {:>5.1}%（{:>4} 点）",
            t,
            roi_m[ti].1 / roi_m[ti].0.max(1.0) * 100.0,
            (roi_m[ti].0 / 100.0) as u64,
            roi_g[ti].1 / roi_g[ti].0.max(1.0) * 100.0,
            (roi_g[ti].0 / 100.0) as u64
        );
    }
    println!(
        "\n[1-2-3着の順序の対数損失, 市場確率で] Harville λ=1: {:.4} / 補正 λ2={} λ3={}: {:.4}",
        t3[0] / cnt,
        harville::LAMBDA2,
        harville::LAMBDA3,
        t3[1] / cnt
    );

    // どの特徴量がオッズにない情報を持つか: 市場 + 各特徴量で当てはめた係数（ローリング検証の各区間で）
    println!("\n[市場と一緒に当てはめた各特徴量の係数（区間ごと, 符号が揃うものが本物）]");
    let mut rows_out: Vec<Vec<f64>> = vec![Vec::new(); ADOPTED.len() + 1 + PACE_NAMES.len()];
    for k in 4..folds {
        let train_r = &races[..k * per];
        let mut std = StdTimes::fit(train_r.iter().flat_map(|r| r.entries.iter().flat_map(|e| e.past_runs.iter())));
        let mut tr: Vec<Vec<Runner>> = Vec::new();
        for r in train_r {
            if let Some(x) = runners(r, &std, *bias.get(&r.race_info.race_id).unwrap_or(&(0.0, 0.0))) {
                tr.push(x);
            }
            std.add_results(&r.race_info, &r.entries);
        }
        let cols = |x: &Runner| -> Vec<f64> {
            let mut v = vec![x.market.max(1e-9).ln()];
            v.extend(ADOPTED.iter().map(|&f| x.feats[f]));
            v.extend(x.pace.iter().copied());
            v
        };
        let b = fit_cols(&tr, &cols, l2);
        for (i, v) in b.iter().enumerate() {
            rows_out[i].push(*v);
        }
    }
    let names: Vec<&str> = std::iter::once("log(市場)")
        .chain(ADOPTED.iter().map(|&f| FEATURE_NAMES[f]))
        .chain(PACE_NAMES.iter().copied())
        .collect();
    for (n, v) in names.iter().zip(&rows_out) {
        println!("  {:<12} {}", n, v.iter().map(|x| format!("{:+.3}", x)).collect::<Vec<_>>().join("  "));
    }

    // アプリに入れる係数と基準タイム表: 全レースで作る
    let mut std = StdTimes::fit(races.iter().flat_map(|r| r.entries.iter().flat_map(|e| e.past_runs.iter())));
    for r in &races {
        std.add_results(&r.race_info, &r.entries);
    }
    if let Some(p) = write_std {
        std::fs::write(&p, std.to_rust_source()).expect("書き込めません");
        println!("\n基準タイム表を書き出しました: {}", p);
    }
    let all: Vec<Vec<Runner>> = races.iter().filter_map(|r| runners(r, &std, *bias.get(&r.race_info.race_id).unwrap_or(&(0.0, 0.0)))).collect();
    // 採用する特徴量（ローリング検証で効いたもの）。使わない特徴量の係数は 0 にする
    let mask = ADOPTED.to_vec();
    let fitted = fit(&all, &mask, l2);
    let mut b = vec![0.0; N_FEATURES];
    for (j, &f) in mask.iter().enumerate() {
        b[f] = fitted[j];
    }
    println!("\n[全 {} レースで当てはめた係数（L2={}）]", all.len(), l2);
    for (n, v) in FEATURE_NAMES.iter().zip(&b) {
        println!("  {:<10} {:+.3}", n, v);
    }
    println!("  MODEL_BETA = {:?}", b.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>());
    // 市場との組み合わせ: u = γ·log(市場勝率) + δ·(モデルの効用)
    let bb = b.clone();
    let comb = move |x: &Runner| -> Vec<f64> {
        vec![x.market.max(1e-9).ln(), (0..N_FEATURES).map(|f| x.feats[f] * bb[f]).sum()]
    };
    let gd = fit_cols(&all, &comb, l2 * 0.25);
    println!("  MARKET_GAMMA = {:.3}, MODEL_DELTA = {:.3}", gd[0], gd[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use netkeiba_predictor_lib::models::PastRun;

    fn info(id: &str, date: &str) -> RaceInfo {
        RaceInfo {
            race_id: id.into(),
            race_name: "テスト".into(),
            race_num: "1R".into(),
            date_str: date.into(),
            venue: "東京".into(),
            grade: None,
            start_time: "10:00".into(),
            surface: "芝".into(),
            distance: 1600,
            direction: "左".into(),
            weather: "晴".into(),
            track_cond: "良".into(),
            head_count: 6,
            is_finished: true,
            source_url: String::new(),
        }
    }

    fn entry(u: u32, rank: Option<u32>, odds: Option<f64>) -> HorseEntry {
        HorseEntry {
            waku: u.min(8),
            umaban: u,
            horse_id: format!("h{}", u),
            horse_name: format!("馬{}", u),
            sex_age: "牡4".into(),
            carried_weight: 57.0,
            jockey_name: String::new(),
            jockey_id: String::new(),
            trainer_name: String::new(),
            sire: String::new(),
            dam: String::new(),
            broodmare_sire: String::new(),
            running_style: if u % 2 == 0 { "先".into() } else { "差".into() },
            interval_text: String::new(),
            body_weight: None,
            weight_diff: None,
            win_odds: odds,
            place_odds_min: None,
            place_odds_max: None,
            popularity: None,
            is_scratched: false,
            past_runs: vec![PastRun {
                date: "2023.01.01".into(),
                venue: "東京".into(),
                race_name: "前走".into(),
                grade: None,
                finish_rank: Some(u),
                surface: "芝".into(),
                distance: 1600,
                time_str: String::new(),
                time_sec: None,
                track_cond: "良".into(),
                head_count: Some(10),
                horse_num: None,
                popularity: None,
                jockey: String::new(),
                carried_weight: None,
                passage: String::new(),
                last_3f: None,
                margin: None,
            }],
            actual_rank: rank,
            actual_time: None,
            actual_last_3f: None,
        }
    }

    fn race(id: &str, entries: Vec<HorseEntry>) -> Race {
        Race { race_info: info(id, "2024年1月14日"), entries, payouts: vec![], combo_odds: ComboOdds::default() }
    }

    fn std0() -> StdTimes {
        StdTimes::fit(std::iter::empty())
    }

    #[test]
    fn non_finishers_count_as_losers_and_stay_in_the_market() {
        // 1番人気（オッズ 1.5）が競走中止で着順なし
        let r = race(
            "202405010101",
            vec![
                entry(1, None, Some(1.5)),
                entry(2, Some(1), Some(5.0)),
                entry(3, Some(2), Some(8.0)),
                entry(4, Some(3), Some(12.0)),
                entry(5, Some(4), Some(20.0)),
            ],
        );
        let rs = runners(&r, &std0(), (0.0, 0.0)).expect("評価に使えるレース");
        assert_eq!(rs.len(), 5, "中止の馬も残す");
        let fav = &rs[0];
        assert_eq!(fav.rank, 99);
        // 市場確率は全出走馬で正規化する（中止の馬を除くと他の馬の確率がふくらむ）
        let inv: f64 = [1.5, 5.0, 8.0, 12.0, 20.0].iter().map(|o| 1.0 / o).sum();
        assert!((fav.market - (1.0 / 1.5) / inv).abs() < 1e-9);
        assert!((rs.iter().map(|x| x.market).sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn races_with_a_runner_missing_odds_are_skipped() {
        let r = race(
            "202405010102",
            vec![
                entry(1, Some(2), None),
                entry(2, Some(1), Some(5.0)),
                entry(3, Some(3), Some(8.0)),
                entry(4, Some(4), Some(12.0)),
                entry(5, Some(5), Some(20.0)),
            ],
        );
        assert!(runners(&r, &std0(), (0.0, 0.0)).is_none());
    }

    #[test]
    fn day_bias_uses_only_earlier_races_of_the_same_day() {
        let mut first = race(
            "202405010101",
            (1..=6).map(|u| entry(u, Some(u), Some(u as f64 * 3.0))).collect(),
        );
        first.race_info.surface = "芝".into();
        let second = race(
            "202405010102",
            (1..=6).map(|u| entry(u, Some(7 - u), Some(u as f64 * 3.0))).collect(),
        );
        let b = day_bias(&[second, first]);
        assert_eq!(b["202405010101"], (0.0, 0.0), "その日の最初のレースには偏りの情報がない");
        let (inner, _) = b["202405010102"];
        assert!(inner < 0.0, "1R は内枠が上位だったので、2R には内有利が入る");
    }

    #[test]
    fn pace_features_ignore_runs_on_or_after_the_race_date() {
        let run = |date: &str, rank: u32| HorseRun {
            date: date.into(),
            head_count: Some(10),
            finish_rank: Some(rank),
            passage: "5-5".into(),
            last3f: Some(34.0),
            race_last3f: Some(35.0),
            last3f_rank: Some(1),
            ..Default::default()
        };
        let mut m = std::collections::HashMap::new();
        m.insert(
            "x".to_string(),
            vec![run("2024/02/01", 1), run("2024/01/14", 1), run("2023/12/01", 10)],
        );
        HORSES.set(m).ok();
        // 2024/01/14 のレースから見ると、使えるのは 2023/12/01 の1走だけ
        let f = pace_features("x", 20240114).expect("過去走がある");
        let only_past = pace_features("x", 20231202).unwrap();
        assert_eq!(f, only_past);
        assert!(pace_features("x", 20231201).is_none(), "当日の走は使わない");
    }
}
