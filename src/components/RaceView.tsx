import { useMemo, useState } from "react";
import { RefreshCw, SlidersHorizontal } from "lucide-react";
import { RacePredictionResult, ScoringWeights } from "../types";
import { byPredictedOrder, pct, surfaceLabel, ticketHorses } from "../lib/format";
import { CapStrip } from "./CapStrip";
import { Cloth } from "./Cloth";
import { EntryTable } from "./EntryTable";
import { TicketPanel } from "./TicketPanel";
import { WeightsPanel } from "./WeightsPanel";

export function RaceView({
  result,
  weights,
  budget,
  strategy,
  onWeights,
  onPreset,
  onBudget,
  onStrategy,
  onSave,
  onRefresh,
  busy,
  memo,
  onMemo,
}: {
  result: RacePredictionResult;
  weights: ScoringWeights;
  budget: number;
  strategy: string;
  onWeights: (w: ScoringWeights) => void;
  onPreset: (w: ScoringWeights) => void;
  onBudget: (b: number) => void;
  onStrategy: (s: string) => void;
  onSave: () => void;
  onRefresh: () => void;
  busy: boolean;
  memo: string;
  onMemo: (m: string) => void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [showWeights, setShowWeights] = useState(false);
  const [sortBy, setSortBy] = useState<"rank" | "umaban">("rank");

  const info = result.race_info;
  const ordered = useMemo(() => byPredictedOrder(result.predictions), [result.predictions]);
  const tickets = useMemo(() => ticketHorses(result), [result]);
  const wakuMap = useMemo(
    () => new Map(result.predictions.map((p) => [p.entry.umaban, p.entry.waku])),
    [result.predictions],
  );
  const rows = sortBy === "rank" ? ordered : [...result.predictions].sort((a, b) => a.entry.umaban - b.entry.umaban);
  const lead = ordered.find((p) => p.mark === "◎");
  const pace = result.pace_analysis;

  return (
    <div className="space-y-4">
      {/* レース名と条件 */}
      <div className="flex flex-wrap items-end gap-x-4 gap-y-2">
        <h1 className="text-[30px] font-black leading-none tracking-tight">{info.race_name}</h1>
        {info.grade && (
          <span className="rounded-sm bg-ink px-1.5 py-0.5 text-[13px] font-black text-waku-5">{info.grade}</span>
        )}
        <span className="text-ink-muted">
          {info.date_str} {info.venue}
          {info.race_num} ・ {surfaceLabel(info.surface)}
          {info.distance}m {info.direction} ・ {info.weather} ・ 馬場 {info.track_cond} ・ {info.head_count}頭 ・{" "}
          {info.start_time}発走
        </span>
        <span
          className={`rounded-sm px-1.5 py-0.5 text-[11px] font-bold ${
            info.is_finished ? "bg-ink text-white" : "border border-ink text-ink"
          }`}
        >
          {info.is_finished ? "確定" : "発走前"}
        </span>
        {!result.pre_race && (
          <span
            className="rounded-sm border border-dashed border-ink-faint px-1.5 py-0.5 text-[11px] text-ink-muted"
            title="確定後のオッズや結果を見た後の予想です。保存しても回収率には含めません"
          >
            確定後の予想・集計外
          </span>
        )}
        <div className="ml-auto flex gap-2">
          <button
            type="button"
            onClick={() => setShowWeights((v) => !v)}
            aria-pressed={showWeights}
            className={`flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-bold ${
              showWeights ? "border-ink bg-ink text-white" : "border-line bg-paper text-ink-soft hover:border-ink"
            }`}
          >
            <SlidersHorizontal className="h-4 w-4" />
            指数の重み
          </button>
          <button
            type="button"
            onClick={onRefresh}
            disabled={busy}
            className="flex items-center gap-1.5 rounded-md border border-line bg-paper px-3 py-1.5 text-[12px] font-bold text-ink-soft hover:border-ink disabled:opacity-50"
            title="netkeibaからオッズと結果を取り直します"
          >
            <RefreshCw className={`h-4 w-4 ${busy ? "animate-spin" : ""}`} />
            取り直す
          </button>
        </div>
      </div>

      {/* 予想順のゼッケン列 */}
      <CapStrip
        ordered={ordered}
        tickets={tickets}
        selected={selected}
        onSelect={(u) => {
          setSelected(u);
          setExpanded(u);
        }}
      />

      {showWeights && <WeightsPanel weights={weights} onChange={onWeights} onPreset={onPreset} />}

      <div className="grid grid-cols-[minmax(0,1fr)_360px] items-start gap-6 max-[1280px]:grid-cols-[minmax(0,1fr)_300px] max-[1280px]:gap-4">
        <section className="min-w-0">
          {lead && (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b-2 border-ink pb-3">
              <span className="text-[40px] font-black leading-none text-mark">◎</span>
              <Cloth waku={lead.entry.waku} umaban={lead.entry.umaban} size="lg" />
              <span className="text-[30px] font-black leading-none">{lead.entry.horse_name}</span>
              <span className="text-ink-muted">
                {lead.entry.sex_age} ・ {lead.entry.jockey_name} {lead.entry.carried_weight.toFixed(1)}
              </span>
              <dl className="ml-auto flex gap-6">
                {[
                  ["指数", lead.total_index.toFixed(1)],
                  ["勝率", pct(lead.win_prob)],
                  ["3着内", pct(lead.top3_prob)],
                  ["単勝", lead.entry.win_odds ? lead.entry.win_odds.toFixed(1) : "-"],
                  ["期待値", lead.expected_value.toFixed(2)],
                ].map(([k, v]) => (
                  <div key={k} className="text-right">
                    <dd className="num text-[26px] font-bold leading-none">{v}</dd>
                    <dt className="text-[11px] text-ink-muted">{k}</dt>
                  </div>
                ))}
              </dl>
            </div>
          )}

          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-[12px]">
            <span className="font-bold">展開 {pace.predicted_pace}</span>
            <span className="text-ink-muted">{pace.favored_style}</span>
            {pace.front_runners.length > 0 && (
              <span className="text-ink-muted">逃げ候補 {pace.front_runners.join("、")}</span>
            )}
            <div className="ml-auto flex items-center gap-1">
              <span className="text-[11px] text-ink-muted">並び</span>
              {(
                [
                  ["rank", "予想順"],
                  ["umaban", "馬番順"],
                ] as const
              ).map(([k, l]) => (
                <button
                  key={k}
                  type="button"
                  aria-pressed={sortBy === k}
                  onClick={() => setSortBy(k)}
                  className={`rounded px-2 py-0.5 font-bold ${sortBy === k ? "bg-ink text-white" : "text-ink-muted hover:text-ink"}`}
                >
                  {l}
                </button>
              ))}
            </div>
          </div>

          <EntryTable
            rows={rows}
            finished={info.is_finished}
            tickets={tickets}
            selected={selected}
            expanded={expanded}
            onSelect={setSelected}
            onToggle={(u) => setExpanded((cur) => (cur === u ? null : u))}
          />
          <p className="pt-3 text-[12px] leading-relaxed text-ink-muted max-w-[72ch]">{pace.commentary}</p>
        </section>

        <div className="sticky top-[72px]">
          <TicketPanel
            result={result}
            wakuOf={(u) => wakuMap.get(u) ?? 1}
            budget={budget}
            strategy={strategy}
            onBudget={onBudget}
            onStrategy={onStrategy}
            onSave={onSave}
            saving={busy}
            memo={memo}
            onMemo={onMemo}
          />
        </div>
      </div>
    </div>
  );
}
