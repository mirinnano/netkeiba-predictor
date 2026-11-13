import { Fragment, useEffect, useRef } from "react";
import { ChevronDown } from "lucide-react";
import { HorsePrediction } from "../types";
import { pct, runningStyle } from "../lib/format";
import { Cloth } from "./Cloth";

// 各要素の寄与をレース内の偏差値にしたもの（50 が平均）。調教は JRA-VAN モデルのときだけ
const BASE_KEYS: { key: keyof HorsePrediction; label: string }[] = [
  { key: "ability_score", label: "能力" },
  { key: "speed_score", label: "時計" },
  { key: "aptitude_score", label: "適性" },
  { key: "jockey_frame_score", label: "騎手" },
  { key: "pace_score", label: "展開" },
];
const TRAINING_KEY: { key: keyof HorsePrediction; label: string } = { key: "training_score", label: "調教" };

function Score({ v }: { v: number }) {
  const strong = v >= 60;
  return (
    <span className={`num inline-block w-7 text-right text-[15px] ${strong ? "font-bold text-ink" : "text-ink-muted"}`}>
      {v.toFixed(0)}
    </span>
  );
}

export function EntryTable({
  rows,
  finished,
  tickets,
  selected,
  expanded,
  onToggle,
  onSelect,
}: {
  rows: HorsePrediction[];
  finished: boolean;
  tickets: Set<number>;
  selected: number | null;
  expanded: number | null;
  onToggle: (umaban: number) => void;
  onSelect: (umaban: number) => void;
}) {
  const rowRefs = useRef(new Map<number, HTMLTableRowElement>());
  useEffect(() => {
    if (selected === null) return;
    rowRefs.current.get(selected)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [selected]);

  const cols = 11 + (finished ? 1 : 0);
  const SCORE_KEYS = rows.some((p) => p.training_score != null) ? [...BASE_KEYS, TRAINING_KEY] : BASE_KEYS;

  return (
    <table className="w-full border-collapse">
      <thead className="sticky top-0 z-10 bg-ground">
        <tr className="text-[11px] text-ink-muted border-b border-ink">
          <th className="py-2 pl-1 pr-2 text-left font-medium w-8">印</th>
          <th className="py-2 px-2 text-left font-medium w-10">馬番</th>
          <th className="py-2 px-2 text-left font-medium">馬名</th>
          <th className="py-2 px-2 text-left font-medium">騎手・斤量</th>
          <th className="py-2 px-2 text-right font-medium">指数</th>
          <th className="py-2 px-2 text-right font-medium whitespace-nowrap max-[1280px]:hidden">
            {SCORE_KEYS.map((s) => (
              <span key={s.label} className="inline-block w-7 text-right">
                {s.label}
              </span>
            ))}
          </th>
          <th className="py-2 px-2 text-right font-medium">勝率</th>
          <th className="py-2 px-2 text-right font-medium max-[1280px]:hidden">3着内</th>
          <th className="py-2 px-2 text-right font-medium">単勝</th>
          <th className="py-2 px-2 text-right font-medium">期待値</th>
          {finished && <th className="py-2 px-2 text-right font-medium">着順</th>}
          <th className="w-7" aria-label="詳細" />
        </tr>
      </thead>
      <tbody>
        {rows.map((p) => {
          const e = p.entry;
          const open = expanded === e.umaban;
          const mine = tickets.has(e.umaban);
          const isSel = selected === e.umaban;
          return (
            <Fragment key={e.umaban}>
              <tr
                ref={(el) => {
                  if (el) rowRefs.current.set(e.umaban, el);
                  else rowRefs.current.delete(e.umaban);
                }}
                tabIndex={0}
                aria-expanded={open}
                onClick={() => {
                  onSelect(e.umaban);
                  onToggle(e.umaban);
                }}
                onKeyDown={(ev) => {
                  if (ev.key === "Enter" || ev.key === " ") {
                    ev.preventDefault();
                    onSelect(e.umaban);
                    onToggle(e.umaban);
                  }
                }}
                className={`border-b border-line-soft cursor-pointer transition-colors ${
                  isSel ? "bg-paper" : "hover:bg-paper/60"
                } ${e.is_scratched ? "text-ink-faint" : ""}`}
              >
                <td className="py-1.5 pl-1 pr-2 text-[17px] font-black text-mark">{p.mark === "消" ? "" : p.mark}</td>
                <td className="py-1.5 px-2">
                  <Cloth waku={e.waku} umaban={e.umaban} size="sm" />
                </td>
                <td className="py-1.5 px-2">
                  <div className="flex items-center gap-1.5">
                    {mine && <span className="w-1.5 h-1.5 rounded-full bg-mine" title="買い目に入っている馬" />}
                    <span className={`font-bold text-[14px] ${e.is_scratched ? "line-through" : ""}`}>{e.horse_name}</span>
                    <span className="text-[11px] text-ink-muted">{e.sex_age}</span>
                    <span className="text-[11px] text-ink-muted">{runningStyle(e.running_style)}</span>
                    {e.is_scratched && <span className="text-[11px] font-bold text-mark">取消</span>}
                  </div>
                  <div className="text-[11px] text-ink-faint truncate max-w-[260px] max-[1280px]:hidden">
                    {e.sire && `父 ${e.sire}`}
                    {e.broodmare_sire && `　母父 ${e.broodmare_sire}`}
                  </div>
                </td>
                <td className="py-1.5 px-2 whitespace-nowrap">
                  <span className="font-medium">{e.jockey_name}</span>
                  <span className="num ml-1.5 text-[14px] text-ink-muted">{e.carried_weight.toFixed(1)}</span>
                  {e.body_weight && (
                    <div className="num text-[12px] text-ink-faint">
                      {e.body_weight}kg
                      {e.weight_diff !== null && ` (${e.weight_diff > 0 ? "+" : ""}${e.weight_diff})`}
                    </div>
                  )}
                </td>
                <td className="py-1.5 px-2 text-right">
                  <span className="num text-[18px] font-bold">{e.is_scratched ? "-" : p.total_index.toFixed(1)}</span>
                </td>
                <td className="py-1.5 px-2 text-right whitespace-nowrap max-[1280px]:hidden">
                  {!e.is_scratched && SCORE_KEYS.map((s) => <Score key={s.label} v={(p[s.key] as number | null) ?? 50} />)}
                </td>
                <td className="py-1.5 px-2 text-right num text-[15px]">{e.is_scratched ? "" : pct(p.win_prob)}</td>
                <td className="py-1.5 px-2 text-right num text-[15px] text-ink-muted max-[1280px]:hidden">
                  {e.is_scratched ? "" : pct(p.top3_prob)}
                </td>
                <td className="py-1.5 px-2 text-right whitespace-nowrap">
                  {e.win_odds ? (
                    <>
                      <span className="num text-[16px] font-bold">{e.win_odds.toFixed(1)}</span>
                      {e.popularity && <span className="ml-1 text-[11px] text-ink-muted">{e.popularity}人気</span>}
                    </>
                  ) : (
                    <span className="text-[11px] text-ink-faint">発売前</span>
                  )}
                </td>
                <td className="py-1.5 px-2 text-right">
                  {!e.is_scratched && (
                    <span
                      className={`num text-[16px] ${p.expected_value >= 1.2 ? "font-bold text-ink" : "text-ink-muted"}`}
                      title={`妙味 ${p.value_rank}`}
                    >
                      {p.expected_value.toFixed(2)}
                    </span>
                  )}
                </td>
                {finished && (
                  <td className="py-1.5 px-2 text-right">
                    {e.actual_rank ? (
                      <span className={`num text-[16px] ${e.actual_rank <= 3 ? "font-bold text-ink" : "text-ink-muted"}`}>
                        {e.actual_rank}着
                      </span>
                    ) : (
                      <span className="text-ink-faint">-</span>
                    )}
                  </td>
                )}
                <td className="py-1.5 pr-1 text-ink-faint">
                  <ChevronDown className={`w-4 h-4 transition-transform ${open ? "rotate-180 text-ink" : ""}`} />
                </td>
              </tr>
              {open && (
                <tr className="bg-paper border-b border-line">
                  <td colSpan={cols} className="px-3 pt-2 pb-3">
                    <Detail p={p} />
                  </td>
                </tr>
              )}
            </Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

function Detail({ p }: { p: HorsePrediction }) {
  const e = p.entry;
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-ink-muted">
        {p.reasons.length > 0 && (
          <span className="text-ink">
            {p.reasons.join("　・　")}
          </span>
        )}
        <span>間隔 {e.interval_text || "不明"}</span>
        {e.dam && <span>母 {e.dam}</span>}
        {e.trainer_name && <span>厩舎 {e.trainer_name}</span>}
      </div>
      {e.past_runs.length === 0 ? (
        <p className="text-[12px] text-ink-faint">過去走のデータがありません（新馬・初出走）。</p>
      ) : (
        <table className="w-full text-[12px]">
          <thead>
            <tr className="text-[11px] text-ink-faint text-left">
              <th className="py-1 pr-3 font-medium">日付・場</th>
              <th className="py-1 pr-3 font-medium">レース</th>
              <th className="py-1 pr-3 font-medium text-right">着順</th>
              <th className="py-1 pr-3 font-medium">コース</th>
              <th className="py-1 pr-3 font-medium text-right">タイム</th>
              <th className="py-1 pr-3 font-medium text-right">上がり</th>
              <th className="py-1 pr-3 font-medium">通過</th>
              <th className="py-1 pr-3 font-medium">騎手</th>
              <th className="py-1 font-medium text-right">着差</th>
            </tr>
          </thead>
          <tbody>
            {e.past_runs.map((r, i) => (
              <tr key={i} className="border-t border-line-soft">
                <td className="py-1 pr-3 whitespace-nowrap text-ink-muted">
                  {r.date} {r.venue}
                </td>
                <td className="py-1 pr-3">
                  {r.race_name}
                  {r.grade && <span className="ml-1 text-[11px] font-bold">{r.grade}</span>}
                </td>
                <td className="py-1 pr-3 text-right num text-[14px]">
                  <span className={r.finish_rank && r.finish_rank <= 3 ? "font-bold" : "text-ink-muted"}>
                    {r.finish_rank ?? "-"}
                  </span>
                  <span className="text-ink-faint">/{r.head_count ?? "-"}</span>
                </td>
                <td className="py-1 pr-3 whitespace-nowrap">
                  {r.surface}
                  {r.distance} {r.track_cond}
                </td>
                <td className="py-1 pr-3 text-right num text-[14px]">{r.time_str || "-"}</td>
                <td className="py-1 pr-3 text-right num text-[14px]">{r.last_3f?.toFixed(1) ?? "-"}</td>
                <td className="py-1 pr-3 num text-[14px] text-ink-muted">{r.passage || "-"}</td>
                <td className="py-1 pr-3 whitespace-nowrap">{r.jockey}</td>
                <td className="py-1 text-right num text-[14px] text-ink-muted">
                  {r.margin !== null ? (r.margin > 0 ? `+${r.margin.toFixed(1)}` : r.margin.toFixed(1)) : "-"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
