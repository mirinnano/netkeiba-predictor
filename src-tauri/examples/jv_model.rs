//! JRA-VAN の全レースから「そのレースより前に分かっていたこと」だけで特徴量を作り、
//! 市場（単勝オッズ）に足して良くなるかを測る。2026年は最終判定用なので読まない。
//! 特徴量はアプリと同じ `jvmodel::Engine` で作る。
//!
//!   cargo run --release --example jv_model -- <keiba.db> [--export src/jv_coef.rs]
//!
//! 情報漏れを防ぐ決まり:
//! - 日付順に1日ずつ処理し、その日の全レースの特徴量を作ってから、その日の結果を履歴に足す。
//! - 基準タイム・ペース・調教の基準は 2021年のデータだけで作る（検証区間は 2023年以降）。
use netkeiba_predictor_lib::jvdata::JvRace;
use netkeiba_predictor_lib::jvmodel::{card_from_jv, started, Engine, ADOPTED, NAMES};
use netkeiba_predictor_lib::jvstore;

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

struct Runner {
    market: f64,
    odds: f64,
    win: bool,
    x: Vec<f64>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().expect("keiba.db を指定してください");
    let export = args.iter().position(|a| a == "--export").and_then(|i| args.get(i + 1)).cloned();
    let conn = jvstore::open(std::path::Path::new(path)).expect("DB を開けません");
    // 2026年は最終判定用なので読まない
    let mut races: Vec<JvRace> = jvstore::load_races(&conn, "20210101", "20260101").expect("レースを読めません");
    races.retain(|r| r.kubun == '7');
    races.sort_by(|a, b| (&a.date, &a.race_id).cmp(&(&b.date, &b.race_id)));
    let works = jvstore::load_works(&conn).expect("調教を読めません");
    let mut engine = Engine::new(&races, works);
    println!("{}", engine.summary());

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
        // 1) その日の特徴量を、前日までの履歴だけで作る
        for r in today.iter().filter(|r| r.surface == "芝" || r.surface == "ダ") {
            let st: Vec<_> = r.runners.iter().filter(|h| started(h)).collect();
            if st.len() < 5 || st.iter().any(|h| h.win_odds.is_none()) || !st.iter().any(|h| h.rank == Some(1)) {
                continue;
            }
            let card = card_from_jv(r);
            for h in &card.runners {
                coverage.0 += 1;
                if engine.has_history(&h.horse_id, card.date) {
                    coverage.1 += 1;
                }
            }
            let x = engine.features(&card);
            let inv: Vec<f64> = st.iter().map(|h| 1.0 / h.win_odds.unwrap()).collect();
            let s: f64 = inv.iter().sum();
            data.push((
                day.clone(),
                st.iter()
                    .enumerate()
                    .map(|(a, h)| Runner { market: inv[a] / s, odds: h.win_odds.unwrap(), win: h.rank == Some(1), x: x[a].clone() })
                    .collect(),
            ));
        }
        // 2) その日の結果を履歴に足す（翌日以降の特徴量にだけ効く）
        engine.add_day(today);
        i = j;
    }
    println!(
        "評価に使えるレース {} / 出走 {} 頭のうち過去走あり {:.1}%",
        data.len(),
        coverage.0,
        coverage.1 as f64 / coverage.0.max(1) as f64 * 100.0
    );
    match export {
        Some(p) => export_coefficients(&data, &p),
        None => evaluate(&data),
    }
}

/// 2021〜2025年の全レースで「市場＋モデル」と「モデルのみ」を当てはめ、アプリ用の係数ファイルを書く
fn export_coefficients(data: &[(String, Vec<Runner>)], path: &str) {
    let all: Vec<&Vec<Runner>> = data.iter().map(|(_, r)| r).collect();
    let cols: Vec<usize> = ADOPTED.to_vec();
    let m = fit(&all, true, &cols, 0.01);
    let s = fit(&all, false, &cols, 0.01);
    let fmt = |v: &[f64]| v.iter().map(|x| format!("{:.6}", x)).collect::<Vec<_>>().join(", ");
    let src = format!(
        "//! jvmodel の係数（自動生成）。`cargo run --release --example jv_model -- <keiba.db> --export src/jv_coef.rs` で作り直す。\n\
         //! 学習: 2021〜2025年の JRA 平地 {} レース、1着の多項ロジット、L2 0.01。\n\
         pub const MARKET_GAMMA: f64 = {:.6};\n\
         pub const MARKET_BETA: [f64; {}] = [{}];\n\
         pub const SOLO_BETA: [f64; {}] = [{}];\n",
        all.len(),
        m[0],
        cols.len(),
        fmt(&m[1..]),
        cols.len(),
        fmt(&s)
    );
    std::fs::write(path, src).expect("書き込めません");
    println!("係数を書き出しました: {}（γ={:.3}）", path, m[0]);
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
