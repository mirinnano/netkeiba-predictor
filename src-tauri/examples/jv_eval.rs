//! JRA-VAN の全レース（2021〜2025年、平地）で「市場に足して良くなる情報か」を測る。
//! 2026年は最終判定用なので読まない（docs/BACKTEST.md）。
//!
//!   cargo run --release --example jv_eval -- <jv_races.jsonl>
use netkeiba_predictor_lib::jvdata::{JvRace, JvRunner};
use std::io::BufRead;

struct Runner {
    market: f64,
    odds: f64,
    win: bool,
    x: Vec<f64>, // レース内で標準化した候補の特徴量
}

const NAMES: [&str; 2] = ["マイニング順位", "マイニングタイム差"];

fn softmax(v: &[f64]) -> Vec<f64> {
    let m = v.iter().cloned().fold(f64::MIN, f64::max);
    let e: Vec<f64> = v.iter().map(|x| (x - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.iter().map(|x| x / s).collect()
}

fn standardize(col: &mut [f64]) {
    let n = col.len() as f64;
    let m = col.iter().sum::<f64>() / n;
    let sd = (col.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n).sqrt();
    for x in col.iter_mut() {
        *x = if sd > 1e-9 { (*x - m) / sd } else { 0.0 };
    }
}

/// 出走した馬（取消・除外を除く）。単勝オッズのない出走馬がいるレースは使わない
fn to_runners(r: &JvRace) -> Option<Vec<Runner>> {
    let started: Vec<&JvRunner> = r
        .runners
        .iter()
        .filter(|h| !matches!(h.abnormal.as_str(), "1" | "2" | "3"))
        .collect();
    if started.len() < 5 || started.iter().any(|h| h.win_odds.is_none()) {
        return None;
    }
    if !started.iter().any(|h| h.rank == Some(1)) {
        return None;
    }
    let inv: Vec<f64> = started.iter().map(|h| 1.0 / h.win_odds.unwrap()).collect();
    let s: f64 = inv.iter().sum();
    // マイニング予想がない馬はレース内の平均で埋める（標準化で 0 になる）
    let mut rank: Vec<f64> = started.iter().map(|h| h.mining_rank.map(|v| -(v as f64)).unwrap_or(f64::NAN)).collect();
    let tmin = started.iter().filter_map(|h| h.mining_time).fold(f64::MAX, f64::min);
    let mut time: Vec<f64> = started.iter().map(|h| h.mining_time.map(|t| tmin - t).unwrap_or(f64::NAN)).collect();
    for col in [&mut rank, &mut time] {
        let ok: Vec<f64> = col.iter().cloned().filter(|v| !v.is_nan()).collect();
        let mean = if ok.is_empty() { 0.0 } else { ok.iter().sum::<f64>() / ok.len() as f64 };
        for v in col.iter_mut() {
            if v.is_nan() {
                *v = mean;
            }
        }
        standardize(col);
    }
    Some(
        started
            .iter()
            .enumerate()
            .map(|(i, h)| Runner {
                market: inv[i] / s,
                odds: h.win_odds.unwrap(),
                win: h.rank == Some(1),
                x: vec![rank[i], time[i]],
            })
            .collect(),
    )
}

/// u = γ·log(市場) + Σ β·x を1着の多項ロジットで当てはめる（β にだけ L2）
fn fit(train: &[Vec<Runner>], cols: &[usize], l2: f64) -> Vec<f64> {
    let k = cols.len() + 1;
    let mut b = vec![0.0; k];
    b[0] = 1.0;
    for _ in 0..400 {
        let mut g = vec![0.0; k];
        for race in train {
            let feat = |r: &Runner| -> Vec<f64> {
                let mut v = vec![r.market.max(1e-9).ln()];
                v.extend(cols.iter().map(|&c| r.x[c]));
                v
            };
            let xs: Vec<Vec<f64>> = race.iter().map(feat).collect();
            let p = softmax(&xs.iter().map(|x| x.iter().zip(&b).map(|(a, c)| a * c).sum()).collect::<Vec<_>>());
            let w = race.iter().position(|r| r.win).unwrap();
            for j in 0..k {
                g[j] += xs[w][j] - p.iter().zip(&xs).map(|(pi, x)| pi * x[j]).sum::<f64>();
            }
        }
        for j in 0..k {
            let reg = if j == 0 { 0.0 } else { l2 * b[j] };
            b[j] += 1.0 * (g[j] / train.len() as f64 - reg);
        }
    }
    b
}

fn probs(race: &[Runner], cols: &[usize], b: &[f64]) -> Vec<f64> {
    softmax(
        &race
            .iter()
            .map(|r| b[0] * r.market.max(1e-9).ln() + cols.iter().zip(&b[1..]).map(|(&c, bb)| bb * r.x[c]).sum::<f64>())
            .collect::<Vec<_>>(),
    )
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
    let mut ms: Vec<f64> = (0..2000)
        .map(|_| (0..n).map(|_| d[(next() % n as u64) as usize]).sum::<f64>() / n as f64)
        .collect();
    ms.sort_by(|x, y| x.partial_cmp(y).unwrap());
    (mean, ms[50], ms[1949])
}

fn main() {
    let path = std::env::args().nth(1).expect("jv_races.jsonl を指定してください");
    let f = std::fs::File::open(&path).unwrap();
    let mut races: Vec<(String, Vec<Runner>)> = Vec::new();
    let mut with_mining = 0;
    for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
        let r: JvRace = serde_json::from_str(&line).unwrap();
        if r.date.as_str() < "20210101" || r.date.as_str() >= "20260101" {
            continue; // 2026年は最終判定用
        }
        if r.kubun != '7' || !(r.surface == "芝" || r.surface == "ダ") {
            continue;
        }
        if r.runners.iter().any(|h| h.mining_rank.is_some()) {
            with_mining += 1;
        }
        if let Some(x) = to_runners(&r) {
            races.push((r.date.clone(), x));
        }
    }
    races.sort_by(|a, b| a.0.cmp(&b.0));
    println!("使えるレース {}（{} 〜 {}）/ マイニング予想のあるレース {}", races.len(), races[0].0, races.last().unwrap().0, with_mining);

    let variants: Vec<(&str, Vec<usize>)> = vec![("市場(γ補正)", vec![]), ("+マイニング順位", vec![0]), ("+マイニングタイム差", vec![1]), ("+両方", vec![0, 1])];
    let folds = 8;
    let per = races.len() / folds;
    let mut pooled: Vec<Vec<f64>> = vec![Vec::new(); variants.len() + 1];
    let mut roi: Vec<[(f64, f64); 3]> = vec![[(0.0, 0.0); 3]; variants.len()];
    const TH: [f64; 3] = [0.9, 1.0, 1.1];
    println!("\n[時系列ローリング検証: 1着の対数損失]");
    for k in 4..folds {
        let train: Vec<Vec<Runner>> = races[..k * per].iter().map(|(_, r)| clone(r)).collect();
        let test = &races[k * per..((k + 1) * per).min(races.len())];
        let mut line = format!("  {}〜 ({}R)", test[0].0, test.len());
        let lm: Vec<f64> = test.iter().map(|(_, r)| loss(r, &r.iter().map(|x| x.market).collect::<Vec<_>>())).collect();
        line += &format!("  市場 {:.4}", lm.iter().sum::<f64>() / lm.len() as f64);
        pooled[0].extend(lm);
        for (vi, (name, cols)) in variants.iter().enumerate() {
            let b = fit(&train, cols, 0.01);
            let mut ls = Vec::new();
            for (_, race) in test {
                let p = probs(race, cols, &b);
                ls.push(loss(race, &p));
                for (i, r) in race.iter().enumerate() {
                    for (ti, t) in TH.iter().enumerate() {
                        if p[i] * r.odds >= *t {
                            roi[vi][ti].0 += 100.0;
                            if r.win {
                                roi[vi][ti].1 += r.odds * 100.0;
                            }
                        }
                    }
                }
            }
            line += &format!("  {} {:.4}", name, ls.iter().sum::<f64>() / ls.len() as f64);
            if k == folds - 1 {
                line += &format!(" β={:?}", b.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>());
            }
            pooled[vi + 1].extend(ls);
        }
        println!("{}", line);
    }
    println!("\n[対応のある比較（検証全区間 {} レース, 負なら左が良い, 95%区間）]", pooled[0].len());
    for vi in 1..variants.len() {
        let (m, lo, hi) = paired_ci(&pooled[vi + 1], &pooled[1]);
        println!("  {} − 市場(γ補正): {:+.4} [{:+.4}, {:+.4}]{}", variants[vi].0, m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
    }
    println!("\n[単勝の回収率（検証区間, 確定オッズ）: 期待値 ≥ 0.9 / 1.0 / 1.1]");
    for (vi, (name, _)) in variants.iter().enumerate() {
        let s: Vec<String> = roi[vi].iter().map(|(b, r)| format!("{:>6.1}%（{}点）", r / b.max(1.0) * 100.0, (*b / 100.0) as u64)).collect();
        println!("  {:<18} {}", name, s.join("  "));
    }
    let _ = NAMES;
}

fn clone(r: &[Runner]) -> Vec<Runner> {
    r.iter().map(|x| Runner { market: x.market, odds: x.odds, win: x.win, x: x.x.clone() }).collect()
}
