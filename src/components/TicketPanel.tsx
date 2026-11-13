import { Save } from "lucide-react";
import { RacePredictionResult } from "../types";
import { pct, yen } from "../lib/format";
import { Flash } from "./Flash";
import { Cloth } from "./Cloth";
import { Segmented } from "./Segmented";

export const STRATEGIES = [
  { value: "balance", label: "バランス" },
  { value: "hit_focus", label: "的中率重視" },
  { value: "value_focus", label: "期待値重視" },
];
export const BUDGETS = [1000, 3000, 5000, 10000];

/** 推定オッズは破線枠の「推定」を付け、実オッズと描き分ける */
export function Odds({ value, estimated }: { value: number; estimated: boolean }) {
  return (
    <span className="whitespace-nowrap">
      {estimated && (
        <span
          className="mr-1 rounded-sm border border-dashed border-ink-faint px-1 text-[10px] text-ink-muted align-middle"
          title="実オッズが取れなかったため、単勝オッズからHarvilleモデルで推定した値です"
        >
          推定
        </span>
      )}
      <span className={`num text-[17px] font-bold ${estimated ? "text-ink-muted" : ""}`}>{value.toFixed(1)}</span>
    </span>
  );
}

export function TicketPanel({
  result,
  wakuOf,
  budget,
  strategy,
  onBudget,
  onStrategy,
  onSave,
  saving,
  memo,
  onMemo,
}: {
  result: RacePredictionResult;
  wakuOf: (umaban: number) => number;
  budget: number;
  strategy: string;
  onBudget: (b: number) => void;
  onStrategy: (s: string) => void;
  onSave: () => void;
  saving: boolean;
  memo: string;
  onMemo: (m: string) => void;
}) {
  const recs = result.recommendations.filter((r) => r.suggested_amount > 0);
  const picks = recs.filter((r) => r.recommended);
  const finished = result.race_info.is_finished;
  return (
    <aside className="flex max-h-[calc(100vh-88px)] flex-col max-md:max-h-none rounded-lg border border-line bg-paper">
      <div className="shrink-0 border-b border-ink px-4 pb-3 pt-3">
        <div className="flex items-baseline justify-between">
          <h2 className="text-[15px] font-black">
            買い目 <span className="ml-1 text-[11px] font-normal text-ink-muted">{recs.length}点</span>
          </h2>
          <span className="num text-[24px] font-bold leading-none">{yen(result.total_bet)}</span>
        </div>
        {picks.length > 0 && (
          <div key={picks.map((p) => p.horses[0]).join(",")} className="rise-in mt-2 rounded-md bg-mine-tint px-3 py-2">
            <div className="text-[11px] font-bold text-mine">おすすめ</div>
            {picks.map((p) => (
              <div key={p.horses.join("-")} className="flex items-baseline justify-between gap-2">
                <span className="flex items-center gap-2">
                  <span className="text-[13px] font-bold">単勝</span>
                  <Cloth waku={wakuOf(p.horses[0])} umaban={p.horses[0]} size="sm" />
                  <span className="truncate text-[14px] font-bold">{p.horse_names[0]}</span>
                </span>
                <Flash value={p.expected_value.toFixed(2)} className="num shrink-0 px-1 text-[20px] font-bold text-mine">
                  期待値 {p.expected_value.toFixed(2)}
                </Flash>
              </div>
            ))}
          </div>
        )}
        {finished && result.total_return !== null && (
          <div className="flex items-baseline justify-between text-[12px]">
            <span className="text-ink-muted">払戻</span>
            <span className={`num text-[17px] font-bold ${result.total_return >= result.total_bet ? "text-hit" : ""}`}>
              {yen(result.total_return)}（{result.roi_percent?.toFixed(1)}%）
            </span>
          </div>
        )}
        <button
          type="button"
          onClick={onSave}
          disabled={saving}
          className="mt-2.5 flex w-full items-center justify-center gap-2 rounded-md bg-shell py-2 text-[13px] font-bold text-white hover:bg-shell-deep disabled:opacity-60"
        >
          <Save className="h-4 w-4" />
          この予想を保存
        </button>
      </div>
      <div className="space-y-2 px-4 pt-2">
        <Segmented label="配分の方針" value={strategy} options={STRATEGIES} onChange={onStrategy} />
        <Segmented
          label="予算"
          value={budget}
          options={BUDGETS.map((b) => ({ value: b, label: b.toLocaleString() }))}
          onChange={onBudget}
        />
      </div>
      <ul className="mt-2 min-h-0 flex-1 overflow-y-auto">
        {recs.map((r, i) => (
          <li
            key={i}
            className={`grid grid-cols-[48px_1fr_auto_56px] items-center gap-x-2 border-t border-line-soft px-4 py-2 ${
              r.is_hit ? "bg-hit/5" : ""
            }`}
          >
            <span className="text-[11px] font-bold text-ink-muted">
              {r.bet_type}
              {r.recommended && (
                <span className="mt-0.5 block w-fit rounded-sm bg-mine px-1 text-[9px] font-bold leading-tight text-white">おすすめ</span>
              )}
            </span>
            <span className="flex gap-0.5" title={r.horse_names.join(" ・ ")}>
              {r.horses.map((h) => (
                <Cloth key={h} waku={wakuOf(h)} umaban={h} size="sm" />
              ))}
            </span>
            <span className="text-right">
              <Odds value={r.est_odds} estimated={r.odds_is_estimated} />
            </span>
            <span className="num text-right text-[17px] font-bold">{r.suggested_amount.toLocaleString()}</span>
            <span className="col-start-2 col-span-3 flex justify-between whitespace-nowrap text-[10px] text-ink-faint">
              <span>
                的中 {pct(r.est_hit_prob)} ・ 期待値 {r.expected_value.toFixed(2)}
              </span>
              {finished && (
                <span className={`font-bold ${r.is_hit ? "text-hit" : ""}`}>
                  {r.is_hit ? `的中 ${yen(r.actual_return ?? 0)}` : "不的中"}
                </span>
              )}
            </span>
          </li>
        ))}
      </ul>
      <label className="block shrink-0 border-t border-line px-4 py-2.5">
        <span className="text-[11px] text-ink-muted">メモ（保存時に一緒に残ります）</span>
        <textarea
          value={memo}
          onChange={(e) => onMemo(e.target.value)}
          rows={2}
          placeholder="回顧や気づき"
          className="mt-0.5 w-full resize-none rounded-md border border-line bg-ground px-2 py-1.5 text-[12px] placeholder:text-ink-faint focus:border-shell focus:outline-none"
        />
      </label>
    </aside>
  );
}
