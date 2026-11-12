use crate::models::{
    BetRecommendation, HorseEntry, HorsePrediction, OfficialPayoutItem, PaceAnalysis, RaceInfo,
    RacePredictionResult, ScoringWeights,
};

pub fn run_prediction(
    race_info: RaceInfo,
    entries: Vec<HorseEntry>,
    official_payouts: Vec<OfficialPayoutItem>,
    weights: ScoringWeights,
    budget: u32,
    strategy: String,
    memo: String,
) -> RacePredictionResult {
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
        });
    }

    // Compute T-score (偏差値 total_index) and Softmax win_prob among active horses
    let active_scores: Vec<f64> = preds
        .iter()
        .filter(|p| !p.entry.is_scratched)
        .map(|p| p.raw_score)
        .collect();

    let n = active_scores.len().max(1) as f64;
    let mean = active_scores.iter().sum::<f64>() / n;
    let variance = active_scores
        .iter()
        .map(|s| (s - mean).powi(2))
        .sum::<f64>()
        / n;
    let std_dev = variance.sqrt().max(4.5);

    // Blend model score with slight market prior if odds are present so win_prob is well-calibrated
    let temp = 8.5;
    let mut exp_weights: Vec<f64> = Vec::with_capacity(preds.len());
    for p in &preds {
        if p.entry.is_scratched {
            exp_weights.push(0.0);
        } else {
            let z = (p.raw_score - mean) / std_dev;
            let t_score = (50.0 + z * 10.0).clamp(25.0, 82.0);
            let w = ((t_score - 50.0) / temp).exp();
            exp_weights.push(w);
        }
    }
    let sum_exp: f64 = exp_weights.iter().sum::<f64>().max(1e-6);

    for (i, p) in preds.iter_mut().enumerate() {
        if p.entry.is_scratched {
            continue;
        }
        let z = (p.raw_score - mean) / std_dev;
        p.total_index = ((50.0 + z * 10.0) * 10.0).round() / 10.0;
        let model_win_prob = (exp_weights[i] / sum_exp).clamp(0.005, 0.75);
        p.win_prob = (model_win_prob * 1000.0).round() / 1000.0;
        p.top3_prob = ((model_win_prob * 2.65).powf(0.82)).clamp(0.02, 0.92);
        p.top3_prob = (p.top3_prob * 1000.0).round() / 1000.0;

        let est_odds = p.entry.win_odds.unwrap_or_else(|| {
            // Estimate fair odds if pre-sale
            (0.80 / p.win_prob.max(0.01)).clamp(1.2, 150.0)
        });
        p.expected_value = ((p.win_prob * est_odds) * 100.0).round() / 100.0;

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
    }
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

    let mut score = weighted_sum / weight_total.max(1.0);

    // Slight market consensus adjustment when official/live odds exist
    if let Some(odds) = entry.win_odds {
        if odds <= 2.0 {
            score += 5.0;
        } else if odds <= 4.5 {
            score += 2.5;
        }
    }

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
) -> Vec<BetRecommendation> {
    let active: Vec<&HorsePrediction> = preds.iter().filter(|p| !p.entry.is_scratched).collect();
    if active.len() < 3 {
        return Vec::new();
    }

    let honmei = active[0]; // ◎
    let taikou = active[1]; // ○
    let tanana = active[2]; // ▲
    let star = active
        .iter()
        .find(|p| p.mark == "☆")
        .copied()
        .unwrap_or_else(|| active.get(3).copied().unwrap_or(tanana));
    let renka1 = active
        .iter()
        .find(|p| p.mark == "△")
        .copied()
        .unwrap_or(star);

    let mut recs = Vec::new();

    // Helper for Harville exact/unordered pair & trio probabilities
    let pair_prob = |a: &HorsePrediction, b: &HorsePrediction| -> f64 {
        let pa = a.win_prob.clamp(0.01, 0.70);
        let pb = b.win_prob.clamp(0.01, 0.70);
        let p_ab = pa * (pb / (1.0 - pa).max(0.05));
        let p_ba = pb * (pa / (1.0 - pb).max(0.05));
        (p_ab + p_ba).clamp(0.005, 0.65)
    };

    let wide_prob = |a: &HorsePrediction, b: &HorsePrediction| -> f64 {
        (pair_prob(a, b) * 2.55).clamp(0.01, 0.80)
    };

    let trio_prob = |a: &HorsePrediction, b: &HorsePrediction, c: &HorsePrediction| -> f64 {
        let p_ab = pair_prob(a, b);
        let pc = c.win_prob.clamp(0.01, 0.50);
        (p_ab * (pc * 2.8)).clamp(0.002, 0.45)
    };

    let est_win_odds = |p: &HorsePrediction| -> f64 {
        p.entry
            .win_odds
            .unwrap_or_else(|| (0.80 / p.win_prob.max(0.01)).clamp(1.3, 120.0))
    };

    // 1. 単勝 (◎ or ☆ depending on strategy)
    let tan_target = if strategy == "value_focus" && star.expected_value > honmei.expected_value {
        star
    } else {
        honmei
    };
    let tan_odds = est_win_odds(tan_target);
    recs.push(BetRecommendation {
        bet_type: "単勝".to_string(),
        horses: vec![tan_target.entry.umaban],
        horse_names: vec![tan_target.entry.horse_name.clone()],
        combination_text: format!("{} ({})", tan_target.entry.umaban, tan_target.mark),
        est_hit_prob: tan_target.win_prob,
        est_odds: (tan_odds * 10.0).round() / 10.0,
        expected_value: ((tan_target.win_prob * tan_odds) * 100.0).round() / 100.0,
        suggested_amount: 0,
        confidence: if tan_target.mark == "◎" {
            "本命勝負".to_string()
        } else {
            "高期待値単勝".to_string()
        },
        is_hit: None,
        actual_payout_per_100: None,
        actual_return: None,
    });

    // 2. 馬連 (◎-○, ◎-▲, ◎-☆)
    let umaren_targets = [
        (honmei, taikou, "本線"),
        (honmei, tanana, "対抗押さえ"),
        (honmei, star, "妙味狙い"),
    ];
    for (a, b, conf) in umaren_targets {
        if a.entry.umaban == b.entry.umaban {
            continue;
        }
        let mut h = vec![a.entry.umaban, b.entry.umaban];
        h.sort();
        if recs
            .iter()
            .any(|r: &BetRecommendation| r.bet_type == "馬連" && r.horses == h)
        {
            continue;
        }
        let prob = pair_prob(a, b);
        let oa = est_win_odds(a);
        let ob = est_win_odds(b);
        let est_o = ((oa * ob * 0.42).clamp(1.5, 500.0) * 10.0).round() / 10.0;
        let ev = ((prob * est_o) * 100.0).round() / 100.0;
        recs.push(BetRecommendation {
            bet_type: "馬連".to_string(),
            horses: h.clone(),
            horse_names: vec![a.entry.horse_name.clone(), b.entry.horse_name.clone()],
            combination_text: format!("{} - {}", h[0], h[1]),
            est_hit_prob: (prob * 1000.0).round() / 1000.0,
            est_odds: est_o,
            expected_value: ev,
            suggested_amount: 0,
            confidence: conf.to_string(),
            is_hit: None,
            actual_payout_per_100: None,
            actual_return: None,
        });
    }

    // 3. ワイド (◎-○, ◎-☆)
    let wide_targets = [(honmei, taikou, "鉄板ワイド"), (honmei, star, "穴ワイド妙味")];
    for (a, b, conf) in wide_targets {
        if a.entry.umaban == b.entry.umaban {
            continue;
        }
        let mut h = vec![a.entry.umaban, b.entry.umaban];
        h.sort();
        if recs
            .iter()
            .any(|r: &BetRecommendation| r.bet_type == "ワイド" && r.horses == h)
        {
            continue;
        }
        let prob = wide_prob(a, b);
        let oa = est_win_odds(a);
        let ob = est_win_odds(b);
        let est_o = ((oa * ob * 0.16).clamp(1.2, 150.0) * 10.0).round() / 10.0;
        let ev = ((prob * est_o) * 100.0).round() / 100.0;
        recs.push(BetRecommendation {
            bet_type: "ワイド".to_string(),
            horses: h.clone(),
            horse_names: vec![a.entry.horse_name.clone(), b.entry.horse_name.clone()],
            combination_text: format!("{} - {}", h[0], h[1]),
            est_hit_prob: (prob * 1000.0).round() / 1000.0,
            est_odds: est_o,
            expected_value: ev,
            suggested_amount: 0,
            confidence: conf.to_string(),
            is_hit: None,
            actual_payout_per_100: None,
            actual_return: None,
        });
    }

    // 4. 3連複 (◎-○-▲, ◎-○-☆, ◎-▲-☆, ◎-○-△)
    let fuku3_targets = [
        (honmei, taikou, tanana, "3連複本線"),
        (honmei, taikou, star, "3連複中穴"),
        (honmei, tanana, star, "3連複高配当"),
        (honmei, taikou, renka1, "3連複押さえ"),
    ];
    for (a, b, c, conf) in fuku3_targets {
        let mut h = vec![a.entry.umaban, b.entry.umaban, c.entry.umaban];
        h.sort();
        h.dedup();
        if h.len() < 3 {
            continue;
        }
        if recs
            .iter()
            .any(|r: &BetRecommendation| r.bet_type == "3連複" && r.horses == h)
        {
            continue;
        }
        let prob = trio_prob(a, b, c);
        let oa = est_win_odds(a);
        let ob = est_win_odds(b);
        let oc = est_win_odds(c);
        let est_o = (((oa * ob * oc).powf(0.68) * 0.45).clamp(2.2, 1200.0) * 10.0).round() / 10.0;
        let ev = ((prob * est_o) * 100.0).round() / 100.0;
        recs.push(BetRecommendation {
            bet_type: "3連複".to_string(),
            horses: h.clone(),
            horse_names: vec![
                a.entry.horse_name.clone(),
                b.entry.horse_name.clone(),
                c.entry.horse_name.clone(),
            ],
            combination_text: format!("{} - {} - {}", h[0], h[1], h[2]),
            est_hit_prob: (prob * 1000.0).round() / 1000.0,
            est_odds: est_o,
            expected_value: ev,
            suggested_amount: 0,
            confidence: conf.to_string(),
            is_hit: None,
            actual_payout_per_100: None,
            actual_return: None,
        });
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
