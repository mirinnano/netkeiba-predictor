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
use std::collections::HashMap;
use netkeiba_predictor_lib::jvmodel::{card_from_jv, started, Engine, ADOPTED, NAMES};
use netkeiba_predictor_lib::jvstore;

const GROUPS: [(&str, &[usize]); 11] = [
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
    ("血統", &[43, 44, 45, 46]),
];

/// 採用中の L2 正則化の強さ（2026-11-13: 0.01 → 0.1。--explore で 0.01 より 0.0005 良かった）
const L2: f64 = 0.1;

struct Runner {
    umaban: u32,
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
    // --from 2011 で 2011年から読む（既定は 2021年）。2026年は最終判定用なので読まない
    let from = args.iter().position(|a| a == "--from").and_then(|i| args.get(i + 1)).map(|y| format!("{}0101", y)).unwrap_or_else(|| "20210101".into());
    let mut races: Vec<JvRace> = jvstore::load_races(&conn, &from, "20260101").expect("レースを読めません");
    races.retain(|r| r.kubun == '7');
    races.sort_by(|a, b| (&a.date, &a.race_id).cmp(&(&b.date, &b.race_id)));
    let works = jvstore::load_works(&conn).expect("調教を読めません");
    let mut engine = Engine::new(&races, works);
    if let Some(um) = args.iter().position(|a| a == "--um").and_then(|i| args.get(i + 1)) {
        let ped = load_pedigree(um);
        println!("血統 {} 頭", ped.len());
        engine.set_pedigree(ped);
    }
    println!("{}", engine.summary());

    let mut data: Vec<(String, Vec<Runner>)> = Vec::new();
    let mut classes: Vec<f64> = Vec::new();
    let mut race_ids: Vec<String> = Vec::new();
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
            // 当日の馬場の偏り: 同じ場・同じ馬場で、このレースより前（R が小さい）の確定レース
            let earlier: Vec<&JvRace> = today
                .iter()
                .filter(|e| e.venue_code == r.venue_code && e.surface == r.surface && e.race_id[10..12] < r.race_id[10..12])
                .collect();
            engine.set_day_bias(netkeiba_predictor_lib::jvmodel::day_bias(&earlier));
            let card = card_from_jv(r);
            for h in &card.runners {
                coverage.0 += 1;
                if engine.has_history(&h.horse_id, card.date) {
                    coverage.1 += 1;
                }
            }
            let x = engine.features(&card);
            classes.push(card.class);
            race_ids.push(r.race_id.clone());
            let inv: Vec<f64> = st.iter().map(|h| 1.0 / h.win_odds.unwrap()).collect();
            let s: f64 = inv.iter().sum();
            data.push((
                day.clone(),
                st.iter()
                    .enumerate()
                    .map(|(a, h)| Runner { umaban: h.umaban, market: inv[a] / s, odds: h.win_odds.unwrap(), win: h.rank == Some(1), x: x[a].clone() })
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
    if args.iter().any(|a| a == "--plan") {
        // RACE フォルダ（払戻・オッズ）を --dir で複数指定できる
        let dirs: Vec<std::path::PathBuf> = args.windows(2).filter(|w| w[0] == "--dir").map(|w| std::path::PathBuf::from(&w[1])).collect();
        plan(&data, &classes, &race_ids, &dirs);
        return;
    }
    if let Some(dir) = args.iter().position(|a| a == "--bets").and_then(|i| args.get(i + 1)) {
        bets(&data, &classes, &race_ids, std::path::Path::new(path), std::path::Path::new(dir));
        return;
    }
    if args.iter().any(|a| a == "--explore") {
        explore(&mut data);
        return;
    }
    if args.iter().any(|a| a == "--wet") {
        wet_test(&data);
        return;
    }
    match export {
        Some(p) => export_coefficients(&data, &p),
        None => evaluate(&data, &classes),
    }
}

/// 2021〜2025年の全レースで「市場＋モデル」と「モデルのみ」を当てはめ、アプリ用の係数ファイルを書く
fn export_coefficients(data: &[(String, Vec<Runner>)], path: &str) {
    // 最初の年は履歴の立ち上がりなので学習に使わない（2011年から読んだときは 2012年から学習）。2021年から読むときは従来どおり全部使う
    let first = data.first().map(|d| d.0[0..4].to_string()).unwrap_or_default();
    let skip_first = first.as_str() < "2021";
    let all: Vec<&Vec<Runner>> = data.iter().filter(|(d, _)| !skip_first || d[0..4] != first).map(|(_, r)| r).collect();
    let from_y = data.iter().find(|(d, _)| !skip_first || d[0..4] != first).map(|d| d.0[0..4].to_string()).unwrap_or_default();
    let cols: Vec<usize> = ADOPTED.to_vec();
    let m = fit(&all, true, &cols, L2);
    let s = fit(&all, false, &cols, L2);
    let fmt = |v: &[f64]| v.iter().map(|x| format!("{:.6}", x)).collect::<Vec<_>>().join(", ");
    let src = format!(
        "//! jvmodel の係数（自動生成）。`cargo run --release --example jv_model -- <keiba.db> --export src/jv_coef.rs` で作り直す。\n\
         //! 学習: {}〜2025年の JRA 平地 {} レース、1着の多項ロジット、L2 {}。アプリの Engine は {} 年から履歴を読むこと（特徴量の定義を揃えるため）。\n\
         pub const MARKET_GAMMA: f64 = {:.6};\n\
         pub const MARKET_BETA: [f64; {}] = [{}];\n\
         pub const SOLO_BETA: [f64; {}] = [{}];\n",
        from_y,
        all.len(),
        L2,
        first,
        m[0],
        cols.len(),
        fmt(&m[1..]),
        cols.len(),
        fmt(&s)
    );
    std::fs::write(path, src).expect("書き込めません");
    println!("係数を書き出しました: {}（γ={:.3}）", path, m[0]);
}

/// JV-Data の UM（競走馬マスタ）から 馬 → (父, 母父) を読む。父は 205 バイト目、母父は 389 バイト目（各10桁）
fn load_pedigree(path: &str) -> std::collections::HashMap<String, (String, String)> {
    let bytes = std::fs::read(path).expect("UM を読めません");
    let mut m = std::collections::HashMap::new();
    for line in bytes.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() < 398 || &line[0..2] != b"UM" {
            continue;
        }
        let f = |p: usize, n: usize| String::from_utf8_lossy(&line[p - 1..p - 1 + n]).trim().to_string();
        let (sire, bms) = (f(205, 10), f(389, 10));
        if sire.chars().all(|c| c == '0') {
            continue;
        }
        m.insert(f(12, 10), (sire, bms));
    }
    m
}

/// データを足さずに試せる改善: 正則化の強さ、市場の非線形、少数の交互作用・二乗項。
/// 採用中（38特徴量, L2 0.01）との差を、同じ時系列ローリング検証で比べる。
fn explore(data: &mut [(String, Vec<Runner>)]) {
    let base_n = NAMES.len();
    // 派生列を足す（index base_n..）: lm², lm×頭数, lm×指数最高3, 指数最高3², 着差², 着順率², 過去人気², 騎手勝率²
    for (_, race) in data.iter_mut() {
        let n = race.len() as f64;
        for r in race.iter_mut() {
            let lm = r.market.max(1e-9).ln();
            let x = r.x.clone();
            r.x.extend([lm * lm, lm * n / 10.0, lm * x[0], x[0] * x[0], x[6] * x[6], x[7] * x[7], x[8] * x[8], x[13] * x[13]]);
        }
    }
    let base: Vec<usize> = (0..38).collect();
    let with = |extra: &[usize]| -> Vec<usize> { base.iter().copied().chain(extra.iter().map(|e| base_n + e)).collect() };
    let variants: Vec<(String, Vec<usize>, f64)> = vec![
        ("旧 (L2 0.01)".into(), base.clone(), 0.01),
        ("L2 0.001".into(), base.clone(), 0.001),
        ("L2 0.003".into(), base.clone(), 0.003),
        ("L2 0.03".into(), base.clone(), 0.03),
        ("L2 0.1".into(), base.clone(), 0.1),
        ("L2 0.2".into(), base.clone(), 0.2),
        ("L2 0.3".into(), base.clone(), 0.3),
        ("L2 0.5".into(), base.clone(), 0.5),
        ("L2 1.0".into(), base.clone(), 1.0),
        ("L2 0.3 +全部".into(), with(&[0, 1, 2, 3, 4, 5, 6, 7]), 0.3),
        ("+市場の二乗".into(), with(&[0]), 0.01),
        ("+市場×頭数".into(), with(&[1]), 0.01),
        ("+市場×指数".into(), with(&[2]), 0.01),
        ("+二乗項5つ".into(), with(&[3, 4, 5, 6, 7]), 0.01),
        ("+全部".into(), with(&[0, 1, 2, 3, 4, 5, 6, 7]), 0.01),
    ];
    let folds = 8;
    let per = data.len() / folds;
    let mut pooled: Vec<Vec<f64>> = vec![Vec::new(); variants.len()];
    for k in 4..folds {
        let train: Vec<&Vec<Runner>> = data[..k * per].iter().map(|(_, r)| r).collect();
        let test = &data[k * per..((k + 1) * per).min(data.len())];
        for (vi, (_, cols, l2)) in variants.iter().enumerate() {
            let b = fit(&train, true, cols, *l2);
            pooled[vi].extend(test.iter().map(|(_, r)| loss(r, &race_probs(r, true, cols, &b))));
        }
        eprintln!("区間 {} 完了", k);
    }
    println!("\n[採用中との差（負なら良い, 95%区間）検証 {} レース]", pooled[0].len());
    for vi in 1..variants.len() {
        let (m, lo, hi) = paired_ci(&pooled[vi], &pooled[0]);
        println!("  {:<18} {:+.4} [{:+.4}, {:+.4}]{}", variants[vi].0, m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
    }
}

// ---- アプリの買い目のバックテスト ----

/// JV-Data のレコードから netkeiba 形式のレース ID（年・場・回・日・R）
fn rec_race_id(b: &[u8]) -> String {
    String::from_utf8_lossy(&b[11..15]).to_string() + &String::from_utf8_lossy(&b[19..27])
}

fn num(b: &[u8], p: usize, n: usize) -> Option<u64> {
    std::str::from_utf8(&b[p - 1..p - 1 + n]).ok()?.trim().parse().ok()
}

/// RACE フォルダの HR（払戻）と O2・O3・O5（確定オッズ）を読む
fn load_bet_data(dir: &std::path::Path) -> (HashMap<String, HashMap<(String, Vec<u32>), u64>>, HashMap<String, netkeiba_predictor_lib::models::ComboOdds>) {
    let lines = |name: &str| -> Vec<Vec<u8>> {
        std::fs::read(dir.join(name)).unwrap_or_default().split(|&c| c == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l).to_vec()).filter(|l| l.len() > 30).collect()
    };
    // 払戻: レコード長 717。単勝 103(13×3) 複勝 142(13×5) 馬連 246(16×3) ワイド 294(16×7) 3連複 550(18×3)
    let mut pay: HashMap<String, (char, HashMap<(String, Vec<u32>), u64>)> = HashMap::new();
    for b in lines("HR.txt") {
        if b.len() < 717 {
            continue;
        }
        let kubun = b[2] as char;
        let mut m = HashMap::new();
        let mut put = |kind: &str, start: usize, w: usize, cnt: usize, horses: usize| {
            for i in 0..cnt {
                let p = start + w * i;
                let hs: Vec<u32> = (0..horses).filter_map(|h| num(&b, p + 2 * h, 2).map(|x| x as u32)).collect();
                if let (true, Some(y)) = (hs.len() == horses && hs.iter().all(|x| *x > 0), num(&b, p + 2 * horses, 9)) {
                    let mut k = hs.clone();
                    k.sort_unstable();
                    m.insert((kind.to_string(), k), y);
                }
            }
        };
        put("単勝", 103, 13, 3, 1);
        put("複勝", 142, 13, 5, 1);
        put("馬連", 246, 16, 3, 2);
        put("ワイド", 294, 16, 7, 2);
        put("3連複", 550, 18, 3, 3);
        let id = rec_race_id(&b);
        if pay.get(&id).map(|(k, _)| kubun >= *k).unwrap_or(true) {
            pay.insert(id, (kubun, m));
        }
    }
    let mut odds: HashMap<String, netkeiba_predictor_lib::models::ComboOdds> = HashMap::new();
    // オッズ: 41 バイト目から。馬連 13×153（組番4 オッズ6）、ワイド 17×153（組番4 最低5 最高5）、3連複 15×816（組番6 オッズ6）。オッズは 10 倍の整数
    for (file, w, cnt, horses, olen) in [("O2.txt", 13, 153, 2, 6), ("O3.txt", 17, 153, 2, 5), ("O5.txt", 15, 816, 3, 6)] {
        for b in lines(file) {
            let e = odds.entry(rec_race_id(&b)).or_default();
            for i in 0..cnt {
                let p = 41 + w * i;
                if p + w - 1 > b.len() {
                    break;
                }
                let hs: Vec<u32> = (0..horses).filter_map(|h| num(&b, p + 2 * h, 2).map(|x| x as u32)).collect();
                if hs.len() != horses || hs.iter().any(|x| *x == 0) {
                    continue;
                }
                if let Some(o) = num(&b, p + 2 * horses, olen).filter(|o| *o > 0) {
                    let key = netkeiba_predictor_lib::models::combo_key(&hs);
                    let v = o as f64 / 10.0;
                    match file {
                        "O2.txt" => e.umaren.insert(key, v),
                        "O3.txt" => e.wide.insert(key, v),
                        _ => e.sanrenpuku.insert(key, v),
                    };
                }
            }
        }
    }
    (pay.into_iter().map(|(k, (_, m))| (k, m)).collect(), odds)
}

/// O1 の複勝オッズの下限（レース → 馬番 → 下限オッズ）。複勝は 268 バイト目から 12×28（馬番2 下限4 上限4 人気2）。オッズは 10 倍の整数
fn load_place_odds(dir: &std::path::Path) -> HashMap<String, HashMap<u32, f64>> {
    let mut m: HashMap<String, HashMap<u32, f64>> = HashMap::new();
    for l in std::fs::read(dir.join("O1.txt")).unwrap_or_default().split(|&c| c == b'\n') {
        let b = l.strip_suffix(b"\r").unwrap_or(l);
        if b.len() < 603 {
            continue;
        }
        let e = m.entry(rec_race_id(b)).or_default();
        for i in 0..28 {
            let p = 268 + 12 * i;
            if let (Some(h), Some(o)) = (num(b, p, 2), num(b, p + 2, 4)) {
                if h > 0 && o > 0 {
                    e.insert(h as u32, o as f64 / 10.0);
                }
            }
        }
    }
    m
}

/// アプリと同じ買い目（予算 3000 円）を、検証区間の各レースで作り、公式の払戻で精算する。
/// 勝率は、そのレースより前の区間だけで当てはめた「市場＋特徴量」モデル。オッズは確定オッズ（実際に買える締切前より有利な可能性がある）
fn bets(data: &[(String, Vec<Runner>)], classes: &[f64], race_ids: &[String], db: &std::path::Path, dir: &std::path::Path) {
    use netkeiba_predictor_lib::{jvapp, models, predictor};
    let (pay, odds) = load_bet_data(dir);
    println!("払戻 {} レース / オッズ {} レース", pay.len(), odds.len());
    let cols: Vec<usize> = ADOPTED.to_vec();
    let folds = 8;
    let per = data.len() / folds;
    // 2026-11-14 以降、買い目は配分の方針に左右されないので 1 通りだけ
    let strategies = ["balance"];
    // (戦略, クラス区分, 券種, 賭け, 払戻) と、レースごとの (戦略, 区分, 賭け, 払戻)
    let mut tickets: Vec<(usize, usize, String, f64, f64)> = Vec::new();
    let mut races: Vec<(usize, usize, f64, f64)> = Vec::new();
    let mut skipped = 0;
    for k in 4..folds {
        let train: Vec<&Vec<Runner>> = data[..k * per].iter().map(|(_, r)| r).collect();
        let b = fit(&train, true, &cols, L2);
        let end = ((k + 1) * per).min(data.len());
        for idx in k * per..end {
            let race = &data[idx].1;
            let id = &race_ids[idx];
            let (Some(p_map), Some(combo)) = (pay.get(id), odds.get(id)) else { skipped += 1; continue };
            let Some((mut info, entries)) = jvapp::race_from_db(db, id) else { skipped += 1; continue };
            info.is_finished = false;
            let p = race_probs(race, true, &cols, &b);
            let jv = models::JvModelOut { probs: race.iter().zip(&p).map(|(r, q)| (r.umaban, *q)).collect(), contrib: HashMap::new() };
            let c = classes[idx];
            let seg = if c < 3.5 { 0 } else if c < 4.9 { 1 } else { 2 };
            for (si, st) in strategies.iter().enumerate() {
                let res = predictor::run_prediction_with(
                    info.clone(), entries.clone(), Vec::new(), models::ScoringWeights::default(), 3000, st.to_string(), String::new(), combo.clone(), Some(&jv),
                );
                let (mut rb, mut rr) = (0.0, 0.0);
                for t in res.recommendations.iter().filter(|t| t.suggested_amount > 0) {
                    let mut key = t.horses.clone();
                    key.sort_unstable();
                    let y = p_map.get(&(t.bet_type.clone(), key)).copied().unwrap_or(0) as f64;
                    let ret = y * t.suggested_amount as f64 / 100.0;
                    let label = if t.confidence == "期待値1.0未満" { format!("{}(1.0未満)", t.bet_type) } else { t.bet_type.clone() };
                    tickets.push((si, seg, label, t.suggested_amount as f64, ret));
                    rb += t.suggested_amount as f64;
                    rr += ret;
                }
                races.push((si, seg, rb, rr));
            }
        }
        eprintln!("区間 {} 完了", k);
    }
    println!("精算できなかったレース {}", skipped);
    let ci = |v: &[(f64, f64)]| -> (f64, f64) {
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let mut rs: Vec<f64> = (0..2000).map(|_| {
            let (mut b, mut r) = (0.0, 0.0);
            for _ in 0..v.len() { let x = v[(next() % v.len() as u64) as usize]; b += x.0; r += x.1; }
            r / b.max(1.0) * 100.0
        }).collect();
        rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (rs[50], rs[1949])
    };
    for (si, st) in strategies.iter().enumerate() {
        println!("\n[配分 {}（予算 3000 円/レース）]", st);
        for (seg, name) in [(9usize, "全体"), (0, "条件戦"), (1, "OP・L"), (2, "重賞")] {
            let v: Vec<(f64, f64)> = races.iter().filter(|x| x.0 == si && (seg == 9 || x.1 == seg) && x.2 > 0.0).map(|x| (x.2, x.3)).collect();
            if v.is_empty() { continue; }
            let (b, r): (f64, f64) = v.iter().fold((0.0, 0.0), |a, x| (a.0 + x.0, a.1 + x.1));
            let hit = v.iter().filter(|x| x.1 > 0.0).count();
            let plus = v.iter().filter(|x| x.1 > x.0).count();
            let (lo, hi) = ci(&v);
            let mut rets: Vec<f64> = v.iter().map(|x| x.1).collect();
            rets.sort_by(|a, b| b.partial_cmp(a).unwrap());
            let top: f64 = rets.iter().take((v.len() / 20).max(1)).sum();
            println!(
                "  {:<6} {:>5} R  回収率 {:>6.1}% [{:.0}%, {:.0}%]  何か的中 {:>3.0}%  プラス収支 {:>3.0}%  払戻の {:.0}% が上位5%のレース",
                name, v.len(), r / b * 100.0, lo, hi, hit as f64 / v.len() as f64 * 100.0, plus as f64 / v.len() as f64 * 100.0, top / r.max(1.0) * 100.0
            );
        }
        for bt in ["単勝", "単勝(1.0未満)", "馬連", "ワイド", "3連複"] {
            let v: Vec<&(usize, usize, String, f64, f64)> = tickets.iter().filter(|x| x.0 == si && x.2 == bt).collect();
            if v.is_empty() { continue; }
            let (b, r) = v.iter().fold((0.0, 0.0), |a, x| (a.0 + x.3, a.1 + x.4));
            let hit = v.iter().filter(|x| x.4 > 0.0).count();
            println!("    {:<4} {:>6} 点  回収率 {:>6.1}%  的中率 {:>4.1}%", bt, v.len(), r / b * 100.0, hit as f64 / v.len() as f64 * 100.0);
        }
    }
}

// ---- 買い方の検証（docs/BACKTEST.md「買い方の検証計画」のとおり） ----

const BET_TYPES: [&str; 5] = ["単勝", "馬連", "ワイド", "3連複", "複勝"];
const THRESHOLDS: [f64; 4] = [1.0, 1.1, 1.2, 1.3];

/// 期待値 1.0 以上の候補 1 点: (年, レース番号, クラス区分, 券種, 期待値, 100円あたり払戻)
type Cand = (u32, usize, usize, usize, f64, f64);

fn plan(data: &[(String, Vec<Runner>)], classes: &[f64], race_ids: &[String], dirs: &[std::path::PathBuf]) {
    use netkeiba_predictor_lib::predictor::harville;
    let mut pay: HashMap<String, HashMap<(String, Vec<u32>), u64>> = HashMap::new();
    let mut odds: HashMap<String, netkeiba_predictor_lib::models::ComboOdds> = HashMap::new();
    let mut place: HashMap<String, HashMap<u32, f64>> = HashMap::new();
    for d in dirs {
        let (p, o) = load_bet_data(d);
        pay.extend(p);
        odds.extend(o);
        place.extend(load_place_odds(d));
    }
    println!("払戻 {} レース / オッズ {} レース", pay.len(), odds.len());
    let cols: Vec<usize> = ADOPTED.to_vec();
    let year = |i: usize| -> u32 { data[i].0[0..4].parse().unwrap_or(0) };
    let mut cands: Vec<Cand> = Vec::new();
    // 較正: (券種, 期間 0=選ぶ 1=確かめる, 区分) → (予測確率の合計, 的中数, 点数)。区分は予測確率の対数で 10 段
    let mut calib: HashMap<(usize, usize, usize), (f64, f64, f64)> = HashMap::new();
    let mut races_in: [usize; 2] = [0, 0];
    for y in 2016..=2025u32 {
        // 2011年は履歴の立ち上がりなので学習に使わない
        let train: Vec<&Vec<Runner>> = (0..data.len()).filter(|&i| year(i) >= 2012 && year(i) < y).map(|i| &data[i].1).collect();
        if train.len() < 1000 {
            continue;
        }
        let b = fit(&train, true, &cols, L2);
        let period = if y <= 2021 { 0 } else { 1 };
        for i in (0..data.len()).filter(|&i| year(i) == y) {
            let race = &data[i].1;
            let id = &race_ids[i];
            let (Some(pm), Some(om)) = (pay.get(id), odds.get(id)) else { continue };
            races_in[period] += 1;
            let p = race_probs(race, true, &cols, &b);
            let h = harville::Model::new(&p);
            let seg = if classes[i] < 3.5 { 0 } else if classes[i] < 4.9 { 1 } else { 2 };
            let n = race.len();
            let mut add = |t: usize, q: f64, o: Option<f64>, horses: Vec<u32>| {
                let mut k = horses.clone();
                k.sort_unstable();
                let paid = pm.get(&(BET_TYPES[t].to_string(), k)).copied().unwrap_or(0) as f64;
                let bin = ((q.max(1e-6).log10() + 4.0) * 2.5).floor().clamp(0.0, 9.0) as usize;
                let e = calib.entry((t, period, bin)).or_default();
                e.0 += q;
                e.1 += if paid > 0.0 { 1.0 } else { 0.0 };
                e.2 += 1.0;
                if let Some(o) = o {
                    if q * o >= 1.0 {
                        cands.push((y, i, seg, t, q * o, paid));
                    }
                }
            };
            // 複勝: 8 頭以上は 3 着内、6〜7 頭は 2 着内（5 頭以下は発売なし）
            if n >= 6 {
                let top = if n >= 8 { h.top3() } else { (0..n).map(|i| p[i] + (0..n).filter(|&j| j != i).map(|j| h.exacta(j, i)).sum::<f64>()).collect() };
                let pm = place.get(id);
                for a in 0..n {
                    add(4, top[a].min(1.0), pm.and_then(|m| m.get(&race[a].umaban)).copied(), vec![race[a].umaban]);
                }
            }
            for a in 0..n {
                add(0, p[a], Some(race[a].odds), vec![race[a].umaban]);
                for c in a + 1..n {
                    let key = netkeiba_predictor_lib::models::combo_key(&[race[a].umaban, race[c].umaban]);
                    add(1, h.quinella(a, c), om.umaren.get(&key).copied(), vec![race[a].umaban, race[c].umaban]);
                    add(2, h.wide(a, c), om.wide.get(&key).copied(), vec![race[a].umaban, race[c].umaban]);
                    for d in c + 1..n {
                        let key3 = netkeiba_predictor_lib::models::combo_key(&[race[a].umaban, race[c].umaban, race[d].umaban]);
                        add(3, h.trio(a, c, d), om.sanrenpuku.get(&key3).copied(), vec![race[a].umaban, race[c].umaban, race[d].umaban]);
                    }
                }
            }
        }
        eprintln!("{} 年 完了（候補 {}）", y, cands.len());
    }
    println!("レース数: 選ぶ期間（2016〜2021）{} / 確かめる期間（2022〜2025）{}", races_in[0], races_in[1]);

    // 1) 較正（選ぶ期間）。上位（確率の高い）区分で、予測と実際が 2 割以上ずれていないか
    println!("\n[較正: 予測した的中確率と実際の的中率（選ぶ期間）]");
    let mut calib_ok = [true; 5];
    for t in 0..5 {
        let mut line = format!("  {:<4}", BET_TYPES[t]);
        let bins: Vec<usize> = (0..10).filter(|b| calib.get(&(t, 0, *b)).map(|e| e.2 >= 200.0).unwrap_or(false)).collect();
        for &b in &bins {
            let e = calib[&(t, 0, b)];
            line += &format!(" | 予測 {:.3} 実際 {:.3} ({:.0}点)", e.0 / e.2, e.1 / e.2, e.2);
        }
        // 上位の 2 区分を判定に使う
        for &b in bins.iter().rev().take(2) {
            let e = calib[&(t, 0, b)];
            let ratio = (e.1 / e.2) / (e.0 / e.2);
            if !(0.8..=1.2).contains(&ratio) {
                calib_ok[t] = false;
            }
        }
        println!("{}{}", line, if calib_ok[t] { "" } else { "  ← ずれが大きい" });
    }

    // 2) ルールを選ぶ（選ぶ期間だけ）
    let race_ci = |v: &[&Cand]| -> (f64, f64, f64, usize) {
        let mut by: HashMap<usize, (f64, f64)> = HashMap::new();
        for c in v {
            let e = by.entry(c.1).or_default();
            e.0 += 100.0;
            e.1 += c.5;
        }
        let rs: Vec<(f64, f64)> = by.into_values().collect();
        let (b, r) = rs.iter().fold((0.0, 0.0), |a, x| (a.0 + x.0, a.1 + x.1));
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let mut bs: Vec<f64> = (0..2000).map(|_| {
            let (mut bb, mut rr) = (0.0, 0.0);
            for _ in 0..rs.len() { let x = rs[(next() % rs.len() as u64) as usize]; bb += x.0; rr += x.1; }
            rr / bb.max(1.0) * 100.0
        }).collect();
        bs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (r / b.max(1.0) * 100.0, bs[50], bs[1949], rs.len())
    };
    println!("\n[ルールを選ぶ（2016〜2021 年）]");
    let mut rule: [Option<f64>; 5] = [None; 5];
    for t in 0..5 {
        let mut best: Option<(f64, f64, f64)> = None;
        for th in THRESHOLDS {
            let v: Vec<&Cand> = cands.iter().filter(|c| c.0 <= 2021 && c.3 == t && c.4 >= th).collect();
            if v.is_empty() { continue; }
            let (roi, lo, hi, nr) = race_ci(&v);
            println!("  {:<4} 期待値 ≥ {:.1}: {:>6} 点 {:>5} レース 回収率 {:>6.1}% [{:.0}%, {:.0}%]", BET_TYPES[t], th, v.len(), nr, roi, lo, hi);
            if v.len() >= 300 && best.map(|b| roi > b.1).unwrap_or(true) {
                best = Some((th, roi, hi));
            }
        }
        rule[t] = match best {
            Some((th, _, hi)) if hi >= 100.0 && calib_ok[t] => Some(th),
            _ => None,
        };
        println!("  → {}: {}", BET_TYPES[t], rule[t].map(|th| format!("期待値 ≥ {:.1} で買う", th)).unwrap_or("買わない".into()));
    }

    // 3) 確かめる（2022〜2025 年、一度だけ）
    println!("\n[確かめる（2022〜2025 年、選んだルールのまま）]");
    let chosen: Vec<&Cand> = cands.iter().filter(|c| c.0 >= 2022 && rule[c.3].map(|th| c.4 >= th).unwrap_or(false)).collect();
    if chosen.is_empty() {
        println!("  買う券種がありません（どの券種も選ぶ期間で条件を満たさなかった）");
        return;
    }
    let (roi, lo, hi, nr) = race_ci(&chosen);
    let hits = chosen.iter().filter(|c| c.5 > 0.0).count();
    println!("  全体: {} 点 / 買ったレース {}（全 {} レースの {:.0}%）/ 的中 {:.1}% / 回収率 {:.1}% [{:.0}%, {:.0}%]",
        chosen.len(), nr, races_in[1], nr as f64 / races_in[1].max(1) as f64 * 100.0, hits as f64 / chosen.len() as f64 * 100.0, roi, lo, hi);
    for t in 0..5 {
        let v: Vec<&Cand> = chosen.iter().copied().filter(|c| c.3 == t).collect();
        if v.is_empty() { continue; }
        let (roi, lo, hi, nr) = race_ci(&v);
        println!("  {:<4} {:>6} 点 {:>5} レース 回収率 {:.1}% [{:.0}%, {:.0}%]", BET_TYPES[t], v.len(), nr, roi, lo, hi);
    }
    for (seg, name) in [(0usize, "条件戦"), (1, "OP・L"), (2, "重賞")] {
        let v: Vec<&Cand> = chosen.iter().copied().filter(|c| c.2 == seg).collect();
        if v.is_empty() { continue; }
        let (roi, lo, hi, nr) = race_ci(&v);
        println!("  {:<6} {:>6} 点 {:>5} レース 回収率 {:.1}% [{:.0}%, {:.0}%]", name, v.len(), nr, roi, lo, hi);
    }
    let verdict = if lo > 100.0 { "利益が出ると言える" } else if lo > 85.0 { "今の買い目（82〜84%）より良いと言える" } else { "今の買い目より良いとは言えない" };
    println!("  判定: {}", verdict);
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

fn evaluate(data: &[(String, Vec<Runner>)], classes: &[f64]) {
    let all: Vec<usize> = (0..NAMES.len()).collect();
    let mut variants: Vec<(String, bool, Vec<usize>)> = vec![
        ("市場(γ補正)".into(), true, vec![]),
        ("モデルのみ(市場なし)".into(), false, all.clone()),
        // 採用している 38 特徴量（調教の詳細 5 つは検証で悪化したので外す）
        ("市場+全特徴量".into(), true, (0..38).collect()),
        ("市場+全特徴量+調教詳細".into(), true, (0..43).collect()),
        ("市場+全特徴量+血統".into(), true, (0..38).chain(43..47).collect()),
        ("市場+全特徴量+当日".into(), true, (0..38).chain(47..49).collect()),
    ];
    for (g, cols) in GROUPS {
        variants.push((format!("市場+{}", g), true, cols.to_vec()));
    }
    let folds = 8;
    let per = data.len() / folds;
    let test_from = 4 * per;
    let mut pooled: Vec<Vec<f64>> = vec![Vec::new(); variants.len() + 1];
    let mut coefs: Vec<Vec<f64>> = Vec::new();
    const TH: [f64; 4] = [0.8, 0.9, 1.0, 1.1];
    let mut roi = [(0.0f64, 0.0f64); 4];
    // クラス別・レースごとの（賭け額, 払戻）。期待値 1.0 以上の単勝を 100 円ずつ
    let mut per_race: Vec<(usize, f64, f64)> = Vec::new();
    println!("\n[時系列ローリング検証: 1着の対数損失]");
    for k in 4..folds {
        let train: Vec<&Vec<Runner>> = data[..k * per].iter().map(|(_, r)| r).collect();
        let test = &data[k * per..((k + 1) * per).min(data.len())];
        let lm: Vec<f64> = test.iter().map(|(_, r)| loss(r, &r.iter().map(|x| x.market).collect::<Vec<_>>())).collect();
        let mut line = format!("  {}〜 ({}R) 市場 {:.4}", test[0].0, test.len(), lm.iter().sum::<f64>() / lm.len() as f64);
        pooled[0].extend(lm);
        for (vi, (name, wm, cols)) in variants.iter().enumerate() {
            let b = fit(&train, *wm, cols, L2);
            let ls: Vec<f64> = test.iter().map(|(_, r)| loss(r, &race_probs(r, *wm, cols, &b))).collect();
            if vi <= 2 {
                line += &format!(" | {} {:.4}", name, ls.iter().sum::<f64>() / ls.len() as f64);
            }
            if vi == 2 {
                coefs.push(b.clone());
                for (j, (_, race)) in test.iter().enumerate() {
                    let p = race_probs(race, true, cols, &b);
                    let c = classes[k * per + j];
                    let seg = if c < 3.5 { 0 } else if c < 4.9 { 1 } else { 2 };
                    let (mut bet, mut ret) = (0.0, 0.0);
                    for (i, r) in race.iter().enumerate() {
                        if p[i] * r.odds >= 1.0 {
                            bet += 100.0;
                            if r.win {
                                ret += r.odds * 100.0;
                            }
                        }
                    }
                    if bet > 0.0 {
                        per_race.push((seg, bet, ret));
                    }
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
        let (m, lo, hi) = paired_ci(&pooled[6], &pooled[3]);
        println!("  当日の馬場の偏りの上積み = (+当日) − 全特徴量: {:+.4} [{:+.4}, {:+.4}]{}", m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
        let (m, lo, hi) = paired_ci(&pooled[5], &pooled[3]);
        println!("  血統の上積み = (+血統) − 全特徴量: {:+.4} [{:+.4}, {:+.4}]{}", m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
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
    // クラス別: 検証区間のレースをクラスで分け、採用モデルと γ補正市場を比べる
    println!("\n[クラス別の比較: 市場+全特徴量 − 市場(γ補正)（負なら良い, 95%区間）]");
    let segs: [(&str, f64, f64); 4] = [("条件戦(新馬〜3勝)", -1.0, 3.5), ("OP・L", 3.5, 4.9), ("GIII", 4.9, 5.5), ("GI・GII", 5.5, 9.0)];
    let n_test = pooled[1].len();
    for (name, lo_c, hi_c) in segs {
        let idx: Vec<usize> = (0..n_test).filter(|&i| { let c = classes[test_from + i]; c > lo_c && c <= hi_c }).collect();
        if idx.is_empty() { continue; }
        let a: Vec<f64> = idx.iter().map(|&i| pooled[3][i]).collect();
        let b: Vec<f64> = idx.iter().map(|&i| pooled[1][i]).collect();
        let mk: Vec<f64> = idx.iter().map(|&i| pooled[0][i]).collect();
        let (m, lo, hi) = paired_ci(&a, &b);
        println!("  {:<18} {:>5}R 市場 {:.4} 差 {:+.4} [{:+.4}, {:+.4}]{}", name, idx.len(), avg(&mk), m, lo, hi, if hi < 0.0 { "  ← 確かに良い" } else { "" });
    }
    println!("\n[期待値 1.0 以上の単勝をクラス別に（レース単位の 95% 区間）]");
    for (seg, name) in [(0usize, "条件戦"), (1, "OP・L"), (2, "重賞")] {
        let v: Vec<&(usize, f64, f64)> = per_race.iter().filter(|x| x.0 == seg).collect();
        if v.is_empty() {
            continue;
        }
        let bet: f64 = v.iter().map(|x| x.1).sum();
        let ret: f64 = v.iter().map(|x| x.2).sum();
        let hit = v.iter().filter(|x| x.2 > 0.0).count();
        let mut rets: Vec<f64> = v.iter().map(|x| x.2).collect();
        rets.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let top: f64 = rets.iter().take((v.len() / 20).max(1)).sum();
        // レースを復元抽出して回収率の区間を出す
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let mut rs: Vec<f64> = (0..2000).map(|_| {
            let (mut b, mut r) = (0.0, 0.0);
            for _ in 0..v.len() { let x = v[(next() % v.len() as u64) as usize]; b += x.1; r += x.2; }
            r / b * 100.0
        }).collect();
        rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "  {:<6} 買ったレース {:>5} / 的中 {:>4}（{:.0}%） / 回収率 {:.1}% [{:.0}%, {:.0}%] / 払戻の {:.0}% が上位5%のレース",
            name, v.len(), hit, hit as f64 / v.len() as f64 * 100.0, ret / bet * 100.0, rs[50], rs[1949], top / ret.max(1.0) * 100.0
        );
    }
    println!("\n[単勝の回収率（市場+全特徴量, 検証区間, 確定オッズ）]");
    for (ti, t) in TH.iter().enumerate() {
        println!("  期待値 ≥ {:.1}: {:.1}%（{} 点）", t, roi[ti].1 / roi[ti].0.max(1.0) * 100.0, (roi[ti].0 / 100.0) as u64);
    }
}

/// 道悪×道悪実績（列 49）の上積みを、事前に決めた手順どおりに一度だけ確かめる（docs/BACKTEST.md）
fn wet_test(data: &[(String, Vec<Runner>)]) {
    let base: Vec<usize> = ADOPTED.to_vec();
    let with: Vec<usize> = base.iter().copied().chain([49]).collect();
    let folds = 8;
    let per = data.len() / folds;
    let (mut lb, mut lw, mut wet_b, mut wet_w) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut coef = Vec::new();
    for k in 4..folds {
        let train: Vec<&Vec<Runner>> = data[..k * per].iter().map(|(_, r)| r).collect();
        let test = &data[k * per..((k + 1) * per).min(data.len())];
        let bb = fit(&train, true, &base, L2);
        let bw = fit(&train, true, &with, L2);
        coef.push(*bw.last().unwrap());
        for (_, r) in test {
            let a = loss(r, &race_probs(r, true, &base, &bb));
            let b = loss(r, &race_probs(r, true, &with, &bw));
            lb.push(a);
            lw.push(b);
            // 参考: その日が稍重以下だったレース（列 49 がどこかで 0 でない）
            if r.iter().any(|x| x.x[49].abs() > 1e-12) {
                wet_b.push(a);
                wet_w.push(b);
            }
        }
        println!("  区間 {} ({}R) 採用中 {:.4} / +道悪 {:.4} / 係数 {:+.4}", k, test.len(),
            lb[lb.len() - test.len()..].iter().sum::<f64>() / test.len() as f64,
            lw[lw.len() - test.len()..].iter().sum::<f64>() / test.len() as f64, coef.last().unwrap());
    }
    let (m, lo, hi) = paired_ci(&lw, &lb);
    println!("\n[判定] (+道悪) − 採用中: {:+.4} [{:+.4}, {:+.4}] ({} レース){}", m, lo, hi, lb.len(), if hi < 0.0 { "  ← 確かに良い（採用）" } else { "  （採用しない）" });
    let (m, lo, hi) = paired_ci(&wet_w, &wet_b);
    println!("[参考・判定に使わない] 稍重以下のレースだけ: {:+.4} [{:+.4}, {:+.4}] ({} レース)", m, lo, hi, wet_b.len());
}
