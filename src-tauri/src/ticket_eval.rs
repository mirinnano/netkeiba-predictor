//! 買い目全体の評価。1点ずつではなく、買い目の組をまとめて見る。
//!
//! 勝率から、補正つき Harville で1〜3着の並び（最大 18×17×16 通り）すべての確率を出し、
//! 並びごとに買い目全体の払戻を足し合わせる。こうすると、同じ馬に依存した馬券どうしの相関が
//! そのまま入る（◎が飛べば全部外れる、など）。
//! 払戻はそのときのオッズ（実オッズか推定）で見積もるので、オッズの分からない馬券は払戻 0 として数え、点数を別に返す。

use crate::models::{combo_key, BetRecommendation, RacePredictionResult};
use crate::predictor::harville;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TicketSetEval {
    /// 買い目の合計額
    pub total_bet: u32,
    /// 何か1点でも当たる確率
    pub hit_prob: f64,
    /// 払戻が購入額を上回る確率
    pub profit_prob: f64,
    /// 払戻の期待値（円）
    pub expected_return: f64,
    /// 払戻の中央値（円）
    pub median_return: f64,
    /// 10% の確率でこれ以上になる払戻（円）
    pub p90_return: f64,
    /// 最も良い並びのときの払戻（円）
    pub max_return: f64,
    /// 回収率の分布: [全部外れ, 〜50%, 〜100%, 〜200%, 〜500%, 500%超] の確率
    pub buckets: Vec<f64>,
    /// オッズが分からず払戻を数えられなかった点数
    pub unknown_odds: u32,
    /// 最も多くの馬券に入っている馬と、その馬を含む点数の割合
    pub key_horse: Option<u32>,
    pub key_share: f64,
    /// 軸の馬が3着以内に来ない確率と、そのときに何か当たる確率
    pub key_miss_prob: f64,
    pub hit_prob_if_key_misses: f64,
}

#[derive(Clone, Copy)]
enum Kind {
    Win,
    Place,
    Quinella,
    Wide,
    Exacta,
    Trio,
    Trifecta,
}

struct Ticket {
    kind: Kind,
    idx: Vec<usize>,
    /// 1点あたりの払戻倍率（不明なら 0）
    odds: f64,
    amount: f64,
}

fn kind_of(bet_type: &str) -> Option<Kind> {
    Some(match bet_type {
        "単勝" => Kind::Win,
        "複勝" => Kind::Place,
        "馬連" => Kind::Quinella,
        "ワイド" => Kind::Wide,
        "馬単" => Kind::Exacta,
        "3連複" => Kind::Trio,
        "3連単" => Kind::Trifecta,
        _ => return None,
    })
}

/// 1〜3着が (a, b, c) のとき、この馬券が当たるか。place_top は複勝の対象着順（7頭以下は2着まで）
fn hits(t: &Ticket, a: usize, b: usize, c: usize, place_top: usize) -> bool {
    let top3 = [a, b, c];
    let in_top = |x: usize, n: usize| top3[..n].contains(&x);
    match t.kind {
        Kind::Win => t.idx[0] == a,
        Kind::Place => in_top(t.idx[0], place_top),
        Kind::Quinella => in_top(t.idx[0], 2) && in_top(t.idx[1], 2),
        Kind::Wide => in_top(t.idx[0], 3) && in_top(t.idx[1], 3),
        Kind::Exacta => t.idx[0] == a && t.idx[1] == b,
        Kind::Trio => t.idx.iter().all(|&x| in_top(x, 3)),
        Kind::Trifecta => t.idx[0] == a && t.idx[1] == b && t.idx[2] == c,
    }
}

/// 自分の馬券があればそれを、なければ金額のある買い目を評価する
pub fn evaluate(result: &RacePredictionResult) -> TicketSetEval {
    let bets: Vec<&BetRecommendation> = if result.my_bets.is_empty() {
        result.recommendations.iter().filter(|b| b.suggested_amount > 0).collect()
    } else {
        result.my_bets.iter().collect()
    };
    let runners: Vec<_> = result.predictions.iter().filter(|p| !p.entry.is_scratched).collect();
    let n = runners.len();
    let mut out = TicketSetEval {
        total_bet: bets.iter().map(|b| b.suggested_amount).sum(),
        buckets: vec![0.0; 6],
        ..Default::default()
    };
    if n < 3 || bets.is_empty() {
        return out;
    }
    let pos = |u: u32| runners.iter().position(|p| p.entry.umaban == u);
    let p = harville::normalize(&runners.iter().map(|r| r.win_prob.max(0.0)).collect::<Vec<_>>());
    let model = harville::Model::new(&p);

    let mut tickets = Vec::new();
    for b in &bets {
        let (Some(kind), Some(idx)) = (kind_of(&b.bet_type), b.horses.iter().map(|&u| pos(u)).collect::<Option<Vec<_>>>()) else {
            out.unknown_odds += 1;
            continue;
        };
        // オッズ: いま取れている実オッズを優先し（自分の馬券は足したときのオッズのままなので）、なければ馬券に付いたもの
        let key = combo_key(&b.horses);
        let ordered_key = b.horses.iter().map(|h| h.to_string()).collect::<Vec<_>>().join("-");
        let e = &runners[idx[0]].entry;
        let live = match kind {
            Kind::Win => e.win_odds.unwrap_or(0.0),
            Kind::Place => e.place_odds_min.unwrap_or(0.0),
            Kind::Quinella => *result.combo_odds.umaren.get(&key).unwrap_or(&0.0),
            Kind::Wide => *result.combo_odds.wide.get(&key).unwrap_or(&0.0),
            Kind::Trio => *result.combo_odds.sanrenpuku.get(&key).unwrap_or(&0.0),
            Kind::Exacta => *result.combo_odds.umatan.get(&ordered_key).unwrap_or(&0.0),
            Kind::Trifecta => *result.combo_odds.sanrentan.get(&ordered_key).unwrap_or(&0.0),
        };
        let odds = if live > 0.0 { live } else { b.est_odds };
        if odds <= 0.0 {
            out.unknown_odds += 1;
        }
        tickets.push(Ticket { kind, idx, odds, amount: b.suggested_amount as f64 });
    }

    // 軸: 最も多くの馬券に入っている馬
    let mut count = vec![0usize; n];
    for t in &tickets {
        for &i in &t.idx {
            count[i] += 1;
        }
    }
    let key = (0..n).max_by_key(|&i| count[i]).filter(|&i| count[i] > 0);
    out.key_horse = key.map(|i| runners[i].entry.umaban);
    out.key_share = key.map(|i| count[i] as f64 / tickets.len().max(1) as f64).unwrap_or(0.0);

    let place_top = if n <= 7 { 2 } else { 3 };
    let bet = out.total_bet as f64;
    let mut outcomes: Vec<(f64, f64)> = Vec::with_capacity(n * (n - 1) * (n - 2));
    let (mut key_miss, mut key_miss_hit) = (0.0, 0.0);
    for a in 0..n {
        for b in 0..n {
            if b == a {
                continue;
            }
            for c in 0..n {
                if c == a || c == b {
                    continue;
                }
                let pr = model.trifecta(a, b, c);
                let mut ret = 0.0;
                let mut any = false;
                for t in &tickets {
                    if hits(t, a, b, c, place_top) {
                        any = true;
                        ret += t.odds * t.amount;
                    }
                }
                if any {
                    out.hit_prob += pr;
                }
                if ret > bet {
                    out.profit_prob += pr;
                }
                if let Some(k) = key {
                    if k != a && k != b && k != c {
                        key_miss += pr;
                        if any {
                            key_miss_hit += pr;
                        }
                    }
                }
                let roi = if bet > 0.0 { ret / bet } else { 0.0 };
                let bucket = if ret <= 0.0 {
                    0
                } else if roi <= 0.5 {
                    1
                } else if roi <= 1.0 {
                    2
                } else if roi <= 2.0 {
                    3
                } else if roi <= 5.0 {
                    4
                } else {
                    5
                };
                out.buckets[bucket] += pr;
                out.expected_return += pr * ret;
                outcomes.push((ret, pr));
            }
        }
    }
    // 並びの確率の合計は 1 だが、補正の正規化で数値誤差が出るので合計で割っておく
    let total: f64 = outcomes.iter().map(|o| o.1).sum();
    if total > 0.0 {
        out.hit_prob /= total;
        out.profit_prob /= total;
        out.expected_return /= total;
        out.buckets.iter_mut().for_each(|b| *b /= total);
        key_miss /= total;
        key_miss_hit /= total;
    }
    out.key_miss_prob = key_miss;
    out.hit_prob_if_key_misses = if key_miss > 0.0 { key_miss_hit / key_miss } else { 0.0 };

    outcomes.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let quantile = |q: f64| {
        let mut acc = 0.0;
        for (ret, pr) in &outcomes {
            acc += pr / total;
            if acc >= q {
                return *ret;
            }
        }
        outcomes.last().map(|o| o.0).unwrap_or(0.0)
    };
    out.median_return = quantile(0.5);
    out.p90_return = quantile(0.9);
    out.max_return = outcomes.last().map(|o| o.0).unwrap_or(0.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predictor::tests::jv_prediction;

    fn bet(bet_type: &str, horses: &[u32], odds: f64, amount: u32) -> BetRecommendation {
        BetRecommendation {
            bet_type: bet_type.into(),
            horses: horses.to_vec(),
            horse_names: vec![],
            combination_text: String::new(),
            est_hit_prob: 0.0,
            est_odds: odds,
            expected_value: 0.0,
            suggested_amount: amount,
            confidence: String::new(),
            is_hit: None,
            actual_payout_per_100: None,
            actual_return: None,
            odds_is_estimated: false,
            recommended: false,
        }
    }

    fn probs(r: &RacePredictionResult) -> Vec<(u32, f64)> {
        let s: f64 = r.predictions.iter().map(|p| p.win_prob).sum();
        r.predictions.iter().map(|p| (p.entry.umaban, p.win_prob / s)).collect()
    }

    #[test]
    fn single_win_ticket_matches_win_prob_and_ev() {
        let mut r = jv_prediction(false);
        let (u, p) = probs(&r)[0];
        // 実オッズがあればそちらを使う（馬券に付いたオッズは足したときの古い値なので）
        let live = r.predictions.iter().find(|x| x.entry.umaban == u).and_then(|x| x.entry.win_odds).filter(|&o| o > 0.0);
        r.my_bets = vec![bet("単勝", &[u], 4.0, 100)];
        let e = evaluate(&r);
        assert!((e.hit_prob - p).abs() < 1e-9, "{} vs {}", e.hit_prob, p);
        let used = live.unwrap_or(4.0);
        assert!((e.expected_return - p * used * 100.0).abs() < 1e-6);
        // 実オッズがなければ、馬券のオッズを使う
        for x in r.predictions.iter_mut() {
            x.entry.win_odds = None;
        }
        let e = evaluate(&r);
        assert!((e.expected_return - p * 400.0).abs() < 1e-6);
        assert_eq!(e.key_horse, Some(u));
        assert!((e.buckets.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn win_on_every_horse_always_hits() {
        let mut r = jv_prediction(false);
        r.my_bets = probs(&r).iter().map(|(u, _)| bet("単勝", &[*u], 2.0, 100)).collect();
        let e = evaluate(&r);
        assert!((e.hit_prob - 1.0).abs() < 1e-9);
        assert!(e.buckets[0] < 1e-9);
    }

    #[test]
    fn tickets_on_one_horse_all_miss_when_it_misses() {
        let mut r = jv_prediction(false);
        let us: Vec<u32> = probs(&r).iter().map(|x| x.0).collect();
        r.my_bets = vec![
            bet("ワイド", &[us[0], us[1]], 5.0, 100),
            bet("ワイド", &[us[0], us[2]], 5.0, 100),
            bet("3連複", &[us[0], us[1], us[2]], 20.0, 100),
        ];
        let e = evaluate(&r);
        assert_eq!(e.key_horse, Some(us[0]));
        assert!((e.key_share - 1.0).abs() < 1e-9);
        assert!(e.key_miss_prob > 0.0);
        assert!(e.hit_prob_if_key_misses.abs() < 1e-12);
    }

    #[test]
    fn trio_hit_prob_matches_model() {
        let mut r = jv_prediction(false);
        let ps = probs(&r);
        r.my_bets = vec![bet("3連複", &[ps[0].0, ps[1].0, ps[2].0], 10.0, 100)];
        let e = evaluate(&r);
        let m = harville::Model::new(&ps.iter().map(|x| x.1).collect::<Vec<_>>());
        assert!((e.hit_prob - m.trio(0, 1, 2)).abs() < 1e-9);
    }

    #[test]
    fn unknown_odds_are_counted() {
        let mut r = jv_prediction(false);
        let ps = probs(&r);
        r.my_bets = vec![bet("馬単", &[ps[0].0, ps[1].0], 0.0, 100)];
        let e = evaluate(&r);
        assert_eq!(e.unknown_odds, 1);
        assert!(e.hit_prob > 0.0);
        assert_eq!(e.expected_return, 0.0);
    }
}
