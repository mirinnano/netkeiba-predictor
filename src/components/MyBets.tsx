import { useState } from "react";
import { Minus, Plus, X } from "lucide-react";
import { BetRecommendation, RacePredictionResult } from "../types";
import { yen } from "../lib/format";
import { Cloth } from "./Cloth";

const TYPES: { type: string; n: number; ordered: boolean }[] = [
  { type: "単勝", n: 1, ordered: false },
  { type: "複勝", n: 1, ordered: false },
  { type: "馬連", n: 2, ordered: false },
  { type: "ワイド", n: 2, ordered: false },
  { type: "馬単", n: 2, ordered: true },
  { type: "3連複", n: 3, ordered: false },
  { type: "3連単", n: 3, ordered: true },
];

const sameTicket = (a: BetRecommendation, type: string, horses: number[]) => {
  if (a.bet_type !== type) return false;
  const ordered = TYPES.find((t) => t.type === type)?.ordered;
  const x = ordered ? a.horses : [...a.horses].sort((p, q) => p - q);
  const y = ordered ? horses : [...horses].sort((p, q) => p - q);
  return x.length === y.length && x.every((v, i) => v === y[i]);
};

/** 自分の馬券の合計と、確定後の払戻。自分の馬券がなければ買い目どおりに買ったとみなす */
export function myTotals(r: RacePredictionResult) {
  const bets = r.my_bets && r.my_bets.length > 0 ? r.my_bets : r.recommendations.filter((b) => b.suggested_amount > 0);
  const bet = bets.reduce((s, b) => s + b.suggested_amount, 0);
  const ret = bets.reduce((s, b) => s + (b.actual_return ?? 0), 0);
  return { bets, bet, ret, own: !!r.my_bets && r.my_bets.length > 0 };
}

function Stepper({ value, onChange, label }: { value: number; onChange: (v: number) => void; label: string }) {
  return (
    <span className="flex items-center" role="group" aria-label={label}>
      <button
        type="button"
        aria-label="100円減らす"
        disabled={value <= 100}
        onClick={() => onChange(value - 100)}
        className="flex h-9 w-8 items-center justify-center rounded-l-md border border-line bg-paper text-ink-soft active:bg-ground disabled:opacity-35"
      >
        <Minus className="h-4 w-4" />
      </button>
      <span className="num flex h-9 min-w-[52px] items-center justify-center border-y border-line bg-paper px-1 text-[17px] font-bold">
        {value.toLocaleString()}
      </span>
      <button
        type="button"
        aria-label="100円増やす"
        onClick={() => onChange(value + 100)}
        className="flex h-9 w-8 items-center justify-center rounded-r-md border border-line bg-paper text-ink-soft active:bg-ground"
      >
        <Plus className="h-4 w-4" />
      </button>
    </span>
  );
}

/** 自分の馬券。おすすめの買い目を写して始め、金額を変えたり、消したり、足したりする */
export function MyBets({
  result,
  wakuOf,
  onChange,
}: {
  result: RacePredictionResult;
  wakuOf: (u: number) => number;
  onChange: (bets: BetRecommendation[]) => void;
}) {
  const bets = result.my_bets ?? [];
  const finished = result.race_info.is_finished;
  const [adding, setAdding] = useState(false);
  const [type, setType] = useState("単勝");
  const [picked, setPicked] = useState<number[]>([]);
  const [amount, setAmount] = useState(100);
  const spec = TYPES.find((t) => t.type === type)!;
  const runners = result.predictions.filter((p) => !p.entry.is_scratched).sort((a, b) => a.entry.umaban - b.entry.umaban);
  const nameOf = (u: number) => result.predictions.find((p) => p.entry.umaban === u)?.entry.horse_name ?? "";

  const startFromRecs = () =>
    onChange(
      result.recommendations
        .filter((r) => r.suggested_amount > 0)
        .map((r) => ({ ...r, is_hit: null, actual_payout_per_100: null, actual_return: null })),
    );

  const add = () => {
    const horses = spec.ordered ? picked : [...picked].sort((a, b) => a - b);
    const i = bets.findIndex((b) => sameTicket(b, type, horses));
    if (i >= 0) {
      onChange(bets.map((b, j) => (j === i ? { ...b, suggested_amount: b.suggested_amount + amount } : b)));
    } else {
      const winOdds = type === "単勝" ? result.predictions.find((p) => p.entry.umaban === horses[0])?.entry.win_odds ?? 0 : 0;
      onChange([
        ...bets,
        {
          bet_type: type,
          horses,
          horse_names: horses.map(nameOf),
          combination_text: `${type} ${horses.join(spec.ordered ? "→" : "-")}`,
          est_hit_prob: 0,
          est_odds: winOdds,
          expected_value: 0,
          suggested_amount: amount,
          confidence: "自分で追加",
          is_hit: null,
          actual_payout_per_100: null,
          actual_return: null,
          odds_is_estimated: false,
          recommended: false,
        },
      ]);
    }
    setPicked([]);
    setAmount(100);
    setAdding(false);
  };

  const toggle = (u: number) =>
    setPicked((cur) => (cur.includes(u) ? cur.filter((x) => x !== u) : cur.length < spec.n ? [...cur, u] : cur));

  if (bets.length === 0) {
    return (
      <button
        type="button"
        onClick={startFromRecs}
        disabled={finished}
        className="h-11 w-full rounded-md border-2 border-mine text-[14px] font-bold text-mine active:bg-mine-tint disabled:opacity-40"
      >
        おすすめから自分の馬券を作る
      </button>
    );
  }

  const { bet, ret } = myTotals(result);
  return (
    <section className="rounded-md border-2 border-mine" aria-label="自分の馬券">
      <div className="flex items-baseline justify-between bg-mine-tint px-3 py-2">
        <h3 className="text-[14px] font-black text-mine">自分の馬券</h3>
        <span className="num text-[22px] font-bold leading-none text-mine">
          {yen(bet)}
          {finished && <span className={`ml-2 text-[15px] ${ret >= bet ? "text-hit" : "text-ink-muted"}`}>→ {yen(ret)}</span>}
        </span>
      </div>
      <ul>
        {bets.map((b, i) => (
          <li key={`${b.bet_type}-${b.horses.join("-")}`} className="flex items-center gap-1.5 border-t border-line-soft px-3 py-2">
            <span className="w-10 shrink-0 text-[12px] font-bold">{b.bet_type}</span>
            <span className="flex min-w-0 flex-1 gap-0.5" title={b.horse_names.join(" ・ ")}>
              {b.horses.map((h) => (
                <Cloth key={h} waku={wakuOf(h)} umaban={h} size="sm" />
              ))}
            </span>
            {finished ? (
              <span className={`num text-[16px] font-bold ${b.is_hit ? "text-hit" : "text-ink-muted"}`}>
                {b.is_hit ? yen(b.actual_return ?? 0) : `${b.suggested_amount.toLocaleString()} 外れ`}
              </span>
            ) : (
              <>
                <Stepper
                  label={`${b.bet_type} ${b.horses.join("-")} の金額`}
                  value={b.suggested_amount}
                  onChange={(v) => onChange(bets.map((x, j) => (j === i ? { ...x, suggested_amount: v } : x)))}
                />
                <button
                  type="button"
                  aria-label="この馬券を消す"
                  onClick={() => onChange(bets.filter((_, j) => j !== i))}
                  className="flex h-9 w-8 shrink-0 items-center justify-center rounded-md text-ink-muted active:bg-ground"
                >
                  <X className="h-4 w-4" />
                </button>
              </>
            )}
          </li>
        ))}
      </ul>

      {!finished && !adding && (
        <div className="flex gap-2 border-t border-line-soft px-3 py-2">
          <button
            type="button"
            onClick={() => setAdding(true)}
            className="flex h-10 flex-1 items-center justify-center gap-1 rounded-md bg-mine text-[13px] font-bold text-white active:opacity-90"
          >
            <Plus className="h-4 w-4" />
            馬券を足す
          </button>
          <button
            type="button"
            onClick={() => onChange([])}
            className="h-10 rounded-md border border-line px-3 text-[12px] font-bold text-ink-muted active:bg-ground"
          >
            やめる
          </button>
        </div>
      )}

      {!finished && adding && (
        <div className="rise-in space-y-2.5 border-t border-line-soft px-3 py-3">
          <div className="flex flex-wrap gap-1.5" role="group" aria-label="券種">
            {TYPES.map((t) => (
              <button
                key={t.type}
                type="button"
                aria-pressed={type === t.type}
                onClick={() => {
                  setType(t.type);
                  setPicked((cur) => cur.slice(0, t.n));
                }}
                className={`h-9 rounded-full border px-3 text-[13px] font-bold ${
                  type === t.type ? "border-mine bg-mine text-white" : "border-line bg-paper text-ink-soft"
                }`}
              >
                {t.type}
              </button>
            ))}
          </div>
          <div className="text-[12px] text-ink-muted">
            {spec.ordered ? `着順どおりに ${spec.n} 頭選ぶ` : `${spec.n} 頭選ぶ`}（{picked.length}/{spec.n}）
          </div>
          <div className="grid grid-cols-6 gap-1.5">
            {runners.map((p) => {
              const k = picked.indexOf(p.entry.umaban);
              return (
                <button
                  key={p.entry.umaban}
                  type="button"
                  aria-pressed={k >= 0}
                  aria-label={`${p.entry.umaban}番 ${p.entry.horse_name}`}
                  onClick={() => toggle(p.entry.umaban)}
                  className={`relative flex h-12 items-center justify-center rounded-md border-2 transition-colors ${
                    k >= 0 ? "border-mine bg-mine-tint" : "border-transparent"
                  }`}
                >
                  <Cloth waku={p.entry.waku} umaban={p.entry.umaban} size="md" />
                  {k >= 0 && spec.ordered && (
                    <span className="absolute -right-1 -top-1 rounded-full bg-mine px-1 text-[10px] font-bold text-white">{k + 1}着</span>
                  )}
                </button>
              );
            })}
          </div>
          <div className="flex items-center justify-between gap-2">
            <Stepper label="金額" value={amount} onChange={setAmount} />
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => {
                  setAdding(false);
                  setPicked([]);
                }}
                className="h-10 rounded-md border border-line px-3 text-[13px] font-bold text-ink-muted"
              >
                戻る
              </button>
              <button
                type="button"
                disabled={picked.length !== spec.n}
                onClick={add}
                className="h-10 rounded-md bg-mine px-4 text-[13px] font-bold text-white disabled:opacity-40"
              >
                足す
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
