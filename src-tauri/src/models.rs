use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KaisaiDateItem {
    pub date: String,       // "20261108"
    pub label: String,      // "11月8日(日)"
    pub is_saturday: bool,
    pub is_sunday: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KaisaiCalendarResponse {
    pub current_target: String,
    pub prev_date: Option<String>,
    pub next_date: Option<String>,
    pub dates: Vec<KaisaiDateItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceSummary {
    pub race_id: String,
    pub race_num: String,      // "11R"
    pub title: String,         // "アルゼンチン共和国杯"
    pub grade: Option<String>, // "GII", "GI", "GIII", "OP", etc.
    pub start_time: String,    // "15:35"
    pub surface: String,       // "芝" | "ダ" | "障"
    pub distance: u32,         // 2500
    pub head_count: u32,       // 18
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenueRaces {
    pub venue_name: String,    // "5回 東京 2日目"
    pub short_venue: String,   // "東京"
    pub weather: String,       // "晴"
    pub turf_cond: String,     // "良"
    pub dirt_cond: String,     // "良"
    pub races: Vec<RaceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastRun {
    pub date: String,           // "2023.10.15"
    pub venue: String,          // "京都"
    pub race_name: String,      // "秋華賞"
    pub grade: Option<String>,  // "GI"
    pub finish_rank: Option<u32>, // 1
    pub surface: String,        // "芝"
    pub distance: u32,          // 2000
    pub time_str: String,       // "2:01.1"
    pub time_sec: Option<f64>,  // 121.1
    pub track_cond: String,     // "稍"
    pub head_count: Option<u32>,// 18
    pub horse_num: Option<u32>, // 6
    pub popularity: Option<u32>,// 1
    pub jockey: String,         // "川田将雅"
    pub carried_weight: Option<f64>, // 55.0
    pub passage: String,        // "6-7-7-3"
    pub last_3f: Option<f64>,   // 33.6
    pub margin: Option<f64>,    // -0.1 (negative means won by 0.1s, positive means lost by X sec)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorseEntry {
    pub waku: u32,
    pub umaban: u32,
    pub horse_id: String,
    pub horse_name: String,
    pub sex_age: String,        // "牝3"
    pub carried_weight: f64,    // 54.0
    pub jockey_name: String,
    pub jockey_id: String,
    pub trainer_name: String,
    pub sire: String,           // 父
    pub dam: String,            // 母
    pub broodmare_sire: String, // 母父
    pub running_style: String,  // "逃", "先", "差", "追", "自"
    pub interval_text: String,  // "中5週"
    pub body_weight: Option<u32>,
    pub weight_diff: Option<i32>,
    pub win_odds: Option<f64>,
    pub place_odds_min: Option<f64>,
    pub place_odds_max: Option<f64>,
    pub popularity: Option<u32>,
    pub is_scratched: bool,     // 取消・除外
    pub past_runs: Vec<PastRun>,
    // Actual result if finished
    pub actual_rank: Option<u32>,
    pub actual_time: Option<String>,
    pub actual_last_3f: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorsePrediction {
    pub entry: HorseEntry,
    pub mark: String,             // "◎", "○", "▲", "☆", "△", ""
    pub rank_order: u32,          // 1..N predicted rank
    pub total_index: f64,         // Deviation score (e.g. 68.5)
    pub raw_score: f64,           // 0..100
    pub ability_score: f64,       // 0..100 基礎能力・過去着順
    pub speed_score: f64,         // 0..100 タイム・上がり3F指数
    pub aptitude_score: f64,      // 0..100 コース・距離・馬場適性
    pub jockey_frame_score: f64,  // 0..100 騎手・枠順・状態
    pub pace_score: f64,          // 0..100 展開・脚質マッチ度
    pub win_prob: f64,            // 0.0..1.0
    pub top3_prob: f64,           // 0.0..1.0
    pub expected_value: f64,      // win_prob * win_odds (e.g. 1.25)
    pub value_rank: String,       // "S" | "A" | "B" | "C"
    pub reasons: Vec<String>,     // Highlight tags/reasons
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub ability: f64,       // Default 30
    pub speed: f64,         // Default 25
    pub aptitude: f64,      // Default 20
    pub jockey_frame: f64,  // Default 15
    pub pace: f64,          // Default 10
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            ability: 30.0,
            speed: 25.0,
            aptitude: 20.0,
            jockey_frame: 15.0,
            pace: 10.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialPayoutItem {
    pub bet_type: String,        // "単勝", "複勝", "馬連", "ワイド", "馬単", "3連複", "3連単"
    pub combination: Vec<u32>,   // [2] or [1, 2] or [1, 2, 17] (ordered for 馬単/3連単, sorted for 馬連/ワイド/3連複)
    pub combination_text: String,// "1 - 2"
    pub payout_yen: u32,         // per 100 yen (e.g., 180)
    pub popularity: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetRecommendation {
    pub bet_type: String,        // "単勝", "複勝", "馬連", "ワイド", "馬単", "3連複"
    pub horses: Vec<u32>,        // umaban list
    pub horse_names: Vec<String>,
    pub combination_text: String,// "馬連 1 - 2"
    pub est_hit_prob: f64,       // 0.0..1.0
    pub est_odds: f64,           // Estimated odds
    pub expected_value: f64,     // est_hit_prob * est_odds
    pub suggested_amount: u32,   // Yen (multiples of 100)
    pub confidence: String,      // "本命厚め" | "妙味狙い" | "押さえ" | "高期待値穴"
    pub is_hit: Option<bool>,    // Some(true/false) if race is finished
    pub actual_payout_per_100: Option<u32>,
    pub actual_return: Option<u32>,
    /// true のとき est_odds は実オッズではなく推定値
    #[serde(default)]
    pub odds_is_estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceInfo {
    pub race_id: String,
    pub race_name: String,
    pub race_num: String,
    pub date_str: String,
    pub venue: String,
    pub grade: Option<String>,
    pub start_time: String,
    pub surface: String,         // "芝" | "ダ" | "障"
    pub distance: u32,
    pub direction: String,       // "右" | "左" | "直"
    pub weather: String,
    pub track_cond: String,      // "良" | "稍" | "重" | "不"
    pub head_count: u32,
    pub is_finished: bool,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaceAnalysis {
    pub predicted_pace: String,  // "ハイペース" | "平均ペース" | "スローペース"
    pub front_runners: Vec<String>,
    pub favored_style: String,   // "差し・追込有利" | "先行・好位有利" | "フラット（能力重視）"
    pub commentary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RacePredictionResult {
    pub race_info: RaceInfo,
    pub weights: ScoringWeights,
    pub budget: u32,
    pub strategy: String,
    pub pace_analysis: PaceAnalysis,
    pub predictions: Vec<HorsePrediction>,
    pub recommendations: Vec<BetRecommendation>,
    pub official_payouts: Vec<OfficialPayoutItem>,
    pub total_bet: u32,
    pub total_return: Option<u32>,
    pub roi_percent: Option<f64>,
    pub memo: String,
    pub saved_at: Option<String>,
    /// レース確定前の情報だけで作った予想なら true。回収率の集計対象になる。
    #[serde(default)]
    pub pre_race: bool,
    #[serde(default)]
    pub combo_odds: ComboOdds,
}

/// 組み合わせ馬券の実オッズ。キーは昇順の馬番を "-" で連結したもの（例: "1-2", "1-2-3"）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComboOdds {
    #[serde(default)]
    pub umaren: std::collections::HashMap<String, f64>,
    /// ワイドは下限オッズ
    #[serde(default)]
    pub wide: std::collections::HashMap<String, f64>,
    #[serde(default)]
    pub sanrenpuku: std::collections::HashMap<String, f64>,
}

pub fn combo_key(horses: &[u32]) -> String {
    let mut h = horses.to_vec();
    h.sort_unstable();
    h.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("-")
}

/// result.html から読んだ1頭分の確定着順
#[derive(Debug, Clone)]
pub struct ResultRow {
    pub umaban: u32,
    pub rank: u32,
    pub time: Option<String>,
    pub last_3f: Option<f64>,
    pub win_odds: Option<f64>,
    pub popularity: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPredictionSummary {
    pub race_id: String,
    pub race_date: String,
    pub venue: String,
    pub race_num: String,
    pub race_name: String,
    pub grade: Option<String>,
    pub surface: String,
    pub distance: u32,
    pub honmei_name: String,
    pub honmei_umaban: u32,
    pub total_bet: u32,
    pub total_return: Option<u32>,
    pub is_verified: bool,
    pub is_pre_race: bool,
    pub memo: String,
    pub saved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiDashboardStats {
    pub total_saved_races: u32,
    /// 確定前に保存し、結果照合まで済んだレース数（回収率の母数）
    pub verified_races: u32,
    /// 確定後に保存したため集計から除外したレース数
    pub backtest_races: u32,
    pub honmei_win_count: u32,
    pub honmei_top3_count: u32,
    pub ticket_hit_races: u32,
    pub total_investment: u32,
    pub total_return: u32,
    pub net_profit: i64,
    pub overall_roi: f64,
    pub recent_history: Vec<SavedPredictionSummary>,
}

/// db.netkeiba.com の競走成績ページの1走分（レースの中身まで含む）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HorseRun {
    pub date: String,            // "2025/07/27"
    pub race_id: String,
    pub venue: String,
    pub race_name: String,
    pub head_count: Option<u32>,
    pub waku: Option<u32>,
    pub umaban: Option<u32>,
    pub odds: Option<f64>,
    pub popularity: Option<u32>,
    pub finish_rank: Option<u32>,
    pub jockey: String,
    pub carried_weight: Option<f64>,
    pub surface: String,         // "芝" | "ダ" | "障"
    pub distance: Option<u32>,
    pub track_cond: String,
    /// netkeiba の馬場指数（無料で見える場合のみ）
    pub track_index: Option<i32>,
    pub time_sec: Option<f64>,
    pub margin: Option<f64>,
    pub passage: String,         // "6-7-7-3"
    /// レース全体の前半3F・後半3F（"36.4-34.0"）
    pub race_first3f: Option<f64>,
    pub race_last3f: Option<f64>,
    /// この馬の上がり3F と、レース内での上がり順位（1〜3位のみ判明）
    pub last3f: Option<f64>,
    pub last3f_rank: Option<u32>,
    pub body_weight: Option<u32>,
}
