import { HorsePrediction, RacePredictionResult } from "../types";

/** 枠番 → ゼッケンの配色（背景・文字） */
export const WAKU_CLOTH: Record<number, string> = {
  1: "bg-waku-1 text-ink",
  2: "bg-waku-2 text-white",
  3: "bg-waku-3 text-white",
  4: "bg-waku-4 text-white",
  5: "bg-waku-5 text-ink",
  6: "bg-waku-6 text-white",
  7: "bg-waku-7 text-white",
  8: "bg-waku-8 text-white",
};

export const clothClass = (waku: number) => WAKU_CLOTH[waku] ?? WAKU_CLOTH[1];

export const yen = (n: number) => `${n.toLocaleString()}円`;
export const pct = (p: number, digits = 1) => `${(p * 100).toFixed(digits)}%`;

export const surfaceLabel = (s: string) => (s === "ダ" ? "ダート" : s === "障" ? "障害" : "芝");

/** 推奨買い目に入っている馬番の集合（予約色で示す） */
export function ticketHorses(result: RacePredictionResult): Set<number> {
  const s = new Set<number>();
  for (const r of result.recommendations) {
    if (r.suggested_amount > 0) r.horses.forEach((h) => s.add(h));
  }
  return s;
}

/** 予想順（取消は末尾） */
export function byPredictedOrder(preds: HorsePrediction[]): HorsePrediction[] {
  return [...preds].sort((a, b) => a.rank_order - b.rank_order || a.entry.umaban - b.entry.umaban);
}

export const STYLE_LABEL: Record<string, string> = {
  逃: "逃げ",
  先: "先行",
  差: "差し",
  追: "追込",
};

export function runningStyle(s: string): string {
  for (const k of Object.keys(STYLE_LABEL)) if (s.includes(k)) return STYLE_LABEL[k];
  return s || "-";
}

/** "2026年11月15日" と "15:40" から発走時刻（ローカル時刻）。読めなければ null */
export function postTime(dateStr: string, start: string): Date | null {
  const d = dateStr.match(/(\d{4})\D+(\d{1,2})\D+(\d{1,2})/);
  const t = start.match(/(\d{1,2}):(\d{2})/);
  if (!d || !t) return null;
  return new Date(+d[1], +d[2] - 1, +d[3], +t[1], +t[2]);
}

/** おすすめ（期待値 1.0 以上の単勝）の馬番。比較用に並べた文字列 */
export const recommendedKey = (r: RacePredictionResult) =>
  r.recommendations
    .filter((t) => t.recommended && t.suggested_amount > 0)
    .map((t) => t.horses.join("-"))
    .sort()
    .join(",");

export const hhmm = (d: Date) => `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
