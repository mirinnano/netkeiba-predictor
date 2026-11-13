export interface KaisaiDateItem {
  date: string;
  label: string;
  is_saturday: boolean;
  is_sunday: boolean;
}

export interface KaisaiCalendarResponse {
  current_target: string;
  prev_date: string | null;
  next_date: string | null;
  dates: KaisaiDateItem[];
}

export interface RaceSummary {
  race_id: string;
  race_num: string;
  title: string;
  grade: string | null;
  start_time: string;
  surface: string;
  distance: number;
  head_count: number;
}

export interface VenueRaces {
  venue_name: string;
  short_venue: string;
  weather: string;
  turf_cond: string;
  dirt_cond: string;
  races: RaceSummary[];
}

export interface PastRun {
  date: string;
  venue: string;
  race_name: string;
  grade: string | null;
  finish_rank: number | null;
  surface: string;
  distance: number;
  time_str: string;
  time_sec: number | null;
  track_cond: string;
  head_count: number | null;
  horse_num: number | null;
  popularity: number | null;
  jockey: string;
  carried_weight: number | null;
  passage: string;
  last_3f: number | null;
  margin: number | null;
}

export interface HorseEntry {
  waku: number;
  umaban: number;
  horse_id: string;
  horse_name: string;
  sex_age: string;
  carried_weight: number;
  jockey_name: string;
  jockey_id: string;
  trainer_name: string;
  sire: string;
  dam: string;
  broodmare_sire: string;
  running_style: string;
  interval_text: string;
  body_weight: number | null;
  weight_diff: number | null;
  win_odds: number | null;
  place_odds_min: number | null;
  place_odds_max: number | null;
  popularity: number | null;
  is_scratched: boolean;
  past_runs: PastRun[];
  actual_rank: number | null;
  actual_time: string | null;
  actual_last_3f: number | null;
}

export interface HorsePrediction {
  entry: HorseEntry;
  mark: string;
  rank_order: number;
  total_index: number;
  raw_score: number;
  ability_score: number;
  speed_score: number;
  aptitude_score: number;
  jockey_frame_score: number;
  pace_score: number;
  win_prob: number;
  top3_prob: number;
  expected_value: number;
  value_rank: string;
  reasons: string[];
  /** 調教の寄与（偏差値）。JRA-VAN モデルのときだけ */
  training_score?: number | null;
}

/** 指数を作っている要素と、このレースでの割合（合計 1。モデルの係数から自動で決まる） */
export interface FactorShare {
  name: string;
  share: number;
}

export interface ScoringWeights {
  ability: number;
  speed: number;
  aptitude: number;
  jockey_frame: number;
  pace: number;
}

export interface OfficialPayoutItem {
  bet_type: string;
  combination: number[];
  combination_text: string;
  payout_yen: number;
  popularity: number | null;
}

export interface BetRecommendation {
  bet_type: string;
  horses: number[];
  horse_names: string[];
  combination_text: string;
  est_hit_prob: number;
  est_odds: number;
  expected_value: number;
  suggested_amount: number;
  confidence: string;
  is_hit: boolean | null;
  actual_payout_per_100: number | null;
  actual_return: number | null;
  /** true のとき est_odds は実オッズではなく推定値 */
  odds_is_estimated: boolean;
}

export interface RaceInfo {
  race_id: string;
  race_name: string;
  race_num: string;
  date_str: string;
  venue: string;
  grade: string | null;
  start_time: string;
  surface: string;
  distance: number;
  direction: string;
  weather: string;
  track_cond: string;
  head_count: number;
  is_finished: boolean;
  source_url: string;
}

export interface PaceAnalysis {
  predicted_pace: string;
  front_runners: string[];
  favored_style: string;
  commentary: string;
}

export interface RacePredictionResult {
  race_info: RaceInfo;
  weights: ScoringWeights;
  budget: number;
  strategy: string;
  pace_analysis: PaceAnalysis;
  predictions: HorsePrediction[];
  recommendations: BetRecommendation[];
  official_payouts: OfficialPayoutItem[];
  total_bet: number;
  total_return: number | null;
  roi_percent: number | null;
  memo: string;
  saved_at: string | null;
  /** 確定前の情報だけで作った予想なら true（回収率の集計対象） */
  pre_race: boolean;
  /** "jravan"（JRA-VAN の履歴で当てはめたモデル）か "basic"（従来の5項目モデル） */
  model_source?: string;
  jv_probs?: [number, number][] | null;
  jv_contrib?: [number, number[]][] | null;
  factor_shares?: FactorShare[];
}

export interface JvStatus {
  path: string | null;
  races: number;
  runners: number;
  last_result_date: string | null;
  latest_card_date: string | null;
}

export interface IngestReport {
  file: string;
  from: number;
  to: number;
  upserted: number;
  deleted: number;
  skipped: number;
}

export interface SavedPredictionSummary {
  race_id: string;
  race_date: string;
  venue: string;
  race_num: string;
  race_name: string;
  grade: string | null;
  surface: string;
  distance: number;
  honmei_name: string;
  honmei_umaban: number;
  total_bet: number;
  total_return: number | null;
  is_verified: boolean;
  is_pre_race: boolean;
  memo: string;
  saved_at: string;
}

export interface RoiDashboardStats {
  total_saved_races: number;
  verified_races: number;
  backtest_races: number;
  honmei_win_count: number;
  honmei_top3_count: number;
  ticket_hit_races: number;
  total_investment: number;
  total_return: number;
  net_profit: number;
  overall_roi: number;
  recent_history: SavedPredictionSummary[];
}
