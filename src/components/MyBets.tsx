import { useState } from "react";
import { Minus, Plus, X } from "lucide-react";
import { BetRecommendation, RacePredictionResult } from "../types";
import { yen } from "../lib/format";
import { Cloth } from "./Cloth";
import { BET_KINDS as TYPES, Method, box, formation, wheel } from "../lib/betBuilder";

const METHODS: { id: Method; label: string }[] = [
  { id: "single", label: "1点" },
  { id: "box", label: "ボックス" },
  { id: "wheel", label: "流し" },
  { id: "formation", label: "フォーメーション" },
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
  saveState = "idle",
}: {
  result: RacePredictionResult;
  wakuOf: (u: number) => number;
  onChange: (bets: BetRecommendation[]) => void;
  saveState?: "idle" | "saving" | "saved" | "error";
}) {
  const bets = result.my_bets ?? [];
  const finished = result.race_info.is_finished;
  const [adding, setAdding] = useState(false);
  const [type, setType] = useState("馬連");
  const [method, setMethod] = useState<Method>("single");
  // 選んだ馬。1点・ボックスは rows[0]、流しは rows[0]=軸 rows[1]=相手、フォーメーションは列ごと
  const [rows, setRows] = useState<number[][]>([[], [], []]);
  // 順序のある券種の流しで、軸を何着に置くか（"multi" は着順を問わない）
  const [axisAt, setAxisAt] = useState<string>("0");
  const [amount, setAmount] = useState(100);
  const spec = TYPES.find((t) => t.type === type)!;
  const runners = result.predictions.filter((p) => !p.entry.is_scratched).sort((a, b) => a.entry.umaban - b.entry.umaban);
  const nameOf = (u: number) => result.predictions.find((p) => p.entry.umaban === u)?.entry.horse_name ?? "";
  const methods = spec.n === 1 ? METHODS.filter((m) => m.id === "single" || m.id === "box") : METHODS;
  const maxAxis = Math.max(1, spec.n - 1);
  // 軸の置き方の選択肢（順序のある券種だけ）
  const axisChoices: { id: string; label: string }[] = !spec.ordered
    ? []
    : rows[0].length >= 2
      ? [
          { id: "0,1", label: "1・2着" },
          { id: "0,2", label: "1・3着" },
          { id: "1,2", label: "2・3着" },
          { id: "multi", label: "マルチ" },
        ]
      : [...Array.from({ length: spec.n }, (_, i) => ({ id: String(i), label: `${i + 1}着` })), { id: "multi", label: "マルチ" }];

  const startFromRecs = () =>
    onChange(
      result.recommendations
        .filter((r) => r.suggested_amount > 0)
        .map((r) => ({ ...r, is_hit: null, actual_payout_per_100: null, actual_return: null })),
    );

  // 今の選び方でできる組み合わせ
  const combos: number[][] = (() => {
    if (method === "single") return rows[0].length === spec.n ? [spec.ordered ? rows[0] : [...rows[0]].sort((a, b) => a - b)] : [];
    if (method === "box") return rows[0].length >= spec.n ? box(spec, rows[0]) : spec.n === 1 ? rows[0].map((h) => [h]) : [];
    if (method === "wheel") {
      const multi = axisAt === "multi";
      const pos = multi ? [] : axisAt.split(",").map(Number);
      return wheel(spec, rows[0], rows[1], pos, multi);
    }
    return formation(spec, rows.slice(0, spec.n));
  })();

  // 組み合わせのオッズ（取れているものだけ）
  const oddsOf = (horses: number[]): number => {
    const key = [...horses].sort((a, b) => a - b).join("-");
    const co = result.combo_odds;
    switch (type) {
      case "単勝":
        return result.predictions.find((p) => p.entry.umaban === horses[0])?.entry.win_odds ?? 0;
      case "複勝":
        return result.predictions.find((p) => p.entry.umaban === horses[0])?.entry.place_odds_min ?? 0;
      case "馬連":
        return co?.umaren?.[key] ?? 0;
      case "ワイド":
        return co?.wide?.[key] ?? 0;
      case "3連複":
        return co?.sanrenpuku?.[key] ?? 0;
      default:
        return 0;
    }
  };

  const reset = () => {
    setRows([[], [], []]);
    setAxisAt("0");
  };

  const add = () => {
    let next = [...bets];
    for (const horses of combos) {
      const i = next.findIndex((b) => sameTicket(b, type, horses));
      if (i >= 0) {
        next = next.map((b, j) => (j === i ? { ...b, suggested_amount: b.suggested_amount + amount } : b));
      } else {
        next.push({
          bet_type: type,
          horses,
          horse_names: horses.map(nameOf),
          combination_text: `${type} ${horses.join(spec.ordered ? "→" : "-")}`,
          est_hit_prob: 0,
          est_odds: oddsOf(horses),
          expected_value: 0,
          suggested_amount: amount,
          confidence: "自分で追加",
          is_hit: null,
          actual_payout_per_100: null,
          actual_return: null,
          odds_is_estimated: false,
          recommended: false,
        });
      }
    }
    onChange(next);
    reset();
    setAmount(100);
    setAdding(false);
  };

  const toggle = (row: number, u: number) => {
    if (method === "wheel" && row === 0) {
      // 軸の頭数が変わったら、軸の着順の選択肢も変わるので初期値に戻す
      const len = rows[0].includes(u) ? rows[0].length - 1 : Math.min(rows[0].length + 1, maxAxis);
      setAxisAt(len >= 2 ? "0,1" : "0");
    }
    setRows((cur) =>
      cur.map((r, i) => {
        if (i !== row) return r;
        if (r.includes(u)) return r.filter((x) => x !== u);
        const limit = method === "single" ? spec.n : method === "wheel" && row === 0 ? maxAxis : 99;
        return r.length < limit ? [...r, u] : r;
      }),
    );
  };

  // どの行に、どんな見出しで馬を選ばせるか
  const pickRows: { label: string }[] =
    method === "wheel"
      ? [{ label: `軸（${maxAxis === 1 ? "1頭" : "2頭まで"}）` }, { label: "相手" }]
      : method === "formation"
        ? Array.from({ length: spec.n }, (_, i) => ({ label: spec.ordered ? `${i + 1}着` : `${i + 1}頭目` }))
        : [{ label: method === "box" ? "選ぶ馬" : spec.ordered ? `着順どおりに ${spec.n} 頭` : `${spec.n} 頭` }];

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
        <h3 className="flex items-baseline gap-2 text-[14px] font-black text-mine">
          自分の馬券
          <span
            role="status"
            className={`text-[11px] font-bold ${saveState === "error" ? "text-mark" : "text-ink-muted"}`}
          >
            {saveState === "saving" ? "保存中…" : saveState === "saved" ? "保存済み" : saveState === "error" ? "保存できません" : ""}
          </span>
        </h3>
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
        <div className="rise-in space-y-3 border-t border-line-soft px-3 py-3">
          <div className="flex flex-wrap gap-1.5" role="group" aria-label="券種">
            {TYPES.map((t) => (
              <button
                key={t.type}
                type="button"
                aria-pressed={type === t.type}
                onClick={() => {
                  setType(t.type);
                  if (t.n === 1 && (method === "wheel" || method === "formation")) setMethod("single");
                  reset();
                }}
                className={`h-9 rounded-full border px-3 text-[13px] font-bold ${
                  type === t.type ? "border-mine bg-mine text-white" : "border-line bg-paper text-ink-soft"
                }`}
              >
                {t.type}
              </button>
            ))}
          </div>
          <div className="grid grid-flow-col auto-cols-fr overflow-hidden rounded-md border border-line" role="group" aria-label="買い方">
            {methods.map((m) => (
              <button
                key={m.id}
                type="button"
                aria-pressed={method === m.id}
                onClick={() => {
                  setMethod(m.id);
                  reset();
                }}
                className={`h-9 text-[12px] font-bold ${method === m.id ? "bg-ink text-white" : "bg-paper text-ink-soft"}`}
              >
                {spec.n === 1 && m.id === "box" ? "まとめて" : m.label}
              </button>
            ))}
          </div>

          {pickRows.map((pr, ri) => (
            <div key={ri}>
              <div className="mb-1 text-[12px] font-bold text-ink-muted">
                {pr.label}
                <span className="ml-1 font-normal">{rows[ri].length > 0 ? `${rows[ri].length}頭` : ""}</span>
              </div>
              <div className="grid grid-cols-6 gap-1.5">
                {runners.map((p) => {
                  const u = p.entry.umaban;
                  const k = rows[ri].indexOf(u);
                  const isAxis = method === "wheel" && ri === 1 && rows[0].includes(u);
                  return (
                    <button
                      key={u}
                      type="button"
                      disabled={isAxis}
                      aria-pressed={k >= 0}
                      aria-label={`${pr.label} ${u}番 ${p.entry.horse_name}`}
                      onClick={() => toggle(ri, u)}
                      className={`relative flex h-11 items-center justify-center rounded-md border-2 transition-colors disabled:opacity-25 ${
                        k >= 0 ? "border-mine bg-mine-tint" : "border-transparent"
                      }`}
                    >
                      <Cloth waku={p.entry.waku} umaban={u} size="md" />
                      {k >= 0 && method === "single" && spec.ordered && (
                        <span className="absolute -right-1 -top-1 rounded-full bg-mine px-1 text-[10px] font-bold text-white">{k + 1}着</span>
                      )}
                    </button>
                  );
                })}
              </div>
              {method === "wheel" && ri === 0 && axisChoices.length > 0 && (
                <div className="mt-1.5 flex flex-wrap gap-1.5" role="group" aria-label="軸の着順">
                  {axisChoices.map((c) => (
                    <button
                      key={c.id}
                      type="button"
                      aria-pressed={axisAt === c.id}
                      onClick={() => setAxisAt(c.id)}
                      className={`h-8 rounded-full border px-3 text-[12px] font-bold ${
                        axisAt === c.id ? "border-ink bg-ink text-white" : "border-line bg-paper text-ink-soft"
                      }`}
                    >
                      {c.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))}

          <div className="flex items-center justify-between gap-2 rounded-md bg-ground px-3 py-2">
            <span className="text-[12px] text-ink-muted">
              <span className="num text-[20px] font-bold text-ink">{combos.length}</span> 点 ×
            </span>
            <Stepper label="1点の金額" value={amount} onChange={setAmount} />
            <span className="num text-[18px] font-bold">{yen(combos.length * amount)}</span>
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => {
                setAdding(false);
                reset();
              }}
              className="h-11 rounded-md border border-line px-4 text-[13px] font-bold text-ink-muted"
            >
              戻る
            </button>
            <button
              type="button"
              disabled={combos.length === 0}
              onClick={add}
              className="h-11 flex-1 rounded-md bg-mine text-[14px] font-bold text-white disabled:opacity-40"
            >
              {combos.length > 0 ? `${combos.length}点を足す` : "馬を選んでください"}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
