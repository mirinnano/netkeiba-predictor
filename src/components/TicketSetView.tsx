import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RacePredictionResult } from "../types";
import { yen } from "../lib/format";
import { myTotals } from "./MyBets";

type Eval = {
  total_bet: number;
  hit_prob: number;
  profit_prob: number;
  expected_return: number;
  median_return: number;
  p90_return: number;
  max_return: number;
  buckets: number[];
  unknown_odds: number;
  key_horse: number | null;
  key_share: number;
  key_miss_prob: number;
  hit_prob_if_key_misses: number;
};

const BUCKETS = [
  { label: "全部外れ", color: "#C9C5B9" },
  { label: "〜50%", color: "#E8D9A8" },
  { label: "〜100%", color: "#F2C200" },
  { label: "〜200%", color: "#7CC86E" },
  { label: "〜500%", color: "#1F8A3B" },
  { label: "500%超", color: "#0F5A26" },
];

const pc = (v: number) => `${(v * 100).toFixed(v < 0.1 ? 1 : 0)}%`;

/**
 * 買い目全体の評価。1〜3着の並びすべての確率から、まとめて当たる確率と払戻の分布を出す。
 * 同じ馬に頼った馬券どうしは一緒に外れるので、1点ずつの的中率を足しても分からないことが見える。
 */
export function TicketSetView({ result }: { result: RacePredictionResult }) {
  const [e, setE] = useState<Eval | null>(null);
  useEffect(() => {
    const id = window.setTimeout(() => {
      invoke<Eval>("evaluate_tickets", { currentResult: result })
        .then(setE)
        .catch(() => setE(null));
    }, 120);
    return () => window.clearTimeout(id);
  }, [result]);

  if (!e || e.total_bet === 0 || result.race_info.is_finished) return null;
  const roi = e.total_bet > 0 ? e.expected_return / e.total_bet : 0;
  // 組み合わせ馬券の期待値は、検証（2016〜2021 年）で期待値 1.0 以上を買っても回収率 67〜79% だった
  const combos = myTotals(result).bets.some((b) => b.bet_type !== "単勝");

  return (
    <section aria-label="買い目全体の見通し" className="rounded-md border border-line bg-ground px-3 py-2.5">
      <div className="grid grid-cols-3 gap-2">
        <div>
          <div className="text-[11px] text-ink-muted">何か当たる</div>
          <div className="num text-[24px] font-bold leading-tight">{pc(e.hit_prob)}</div>
        </div>
        <div>
          <div className="text-[11px] text-ink-muted">プラスになる</div>
          <div className="num text-[24px] font-bold leading-tight">{pc(e.profit_prob)}</div>
        </div>
        <div>
          <div className="text-[11px] text-ink-muted">払戻の期待値</div>
          <div className={`num text-[24px] font-bold leading-tight ${roi >= 1 && !combos ? "text-hit" : ""}`}>{pc(roi)}</div>
        </div>
      </div>

      {/* 回収率の分布 */}
      <div className="mt-2 flex h-3 overflow-hidden rounded-full" role="img" aria-label={BUCKETS.map((b, i) => `${b.label} ${pc(e.buckets[i] ?? 0)}`).join("、")}>
        {BUCKETS.map((b, i) => (
          <span key={b.label} style={{ width: `${(e.buckets[i] ?? 0) * 100}%`, background: b.color }} />
        ))}
      </div>
      <div className="mt-1 flex flex-wrap gap-x-2.5 gap-y-0.5 text-[10px] text-ink-muted">
        {BUCKETS.map((b, i) =>
          (e.buckets[i] ?? 0) >= 0.005 ? (
            <span key={b.label} className="flex items-center gap-1">
              <span className="inline-block h-2 w-2 rounded-full" style={{ background: b.color }} />
              {b.label} {pc(e.buckets[i])}
            </span>
          ) : null,
        )}
      </div>

      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-0.5 whitespace-nowrap text-[11px] text-ink-muted">
        <span>
          中央値 <span className="num text-[14px] font-bold text-ink">{yen(Math.round(e.median_return))}</span>
        </span>
        <span>
          上位1割 <span className="num text-[14px] font-bold text-ink">{yen(Math.round(e.p90_return))}</span>
        </span>
        <span>
          最高 <span className="num text-[14px] font-bold text-ink">{yen(Math.round(e.max_return))}</span>
        </span>
      </div>

      {e.key_horse !== null && e.key_share >= 0.5 && (
        <p className="mt-2 text-[12px] leading-snug">
          <span className="font-bold">{e.key_horse}番</span>が{pc(e.key_share)}の馬券に入っています。3着を外す確率は
          <span className="num font-bold"> {pc(e.key_miss_prob)}</span>
          {e.hit_prob_if_key_misses < 0.005 ? "で、そのときは全部外れます。" : `、そのとき何か当たる確率は ${pc(e.hit_prob_if_key_misses)}。`}
        </p>
      )}
      {combos && roi >= 1 && (
        <p className="mt-1 text-[11px] text-warn">組み合わせ馬券の期待値は、過去の検証では期待値どおりに回収できていません。</p>
      )}
      {e.unknown_odds > 0 && (
        <p className="mt-1 text-[11px] text-warn">オッズの分からない {e.unknown_odds} 点は、払戻 0 円として数えています。</p>
      )}
    </section>
  );
}
