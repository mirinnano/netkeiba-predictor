import { RacePredictionResult } from "../types";
import { pct, yen } from "../lib/format";
import { Cloth } from "./Cloth";
import { Odds } from "./TicketPanel";

export function BetsView({ result }: { result: RacePredictionResult }) {
  const info = result.race_info;
  const wakuOf = (u: number) => result.predictions.find((p) => p.entry.umaban === u)?.entry.waku ?? 1;
  const recs = result.recommendations.filter((r) => r.suggested_amount > 0);
  const anyEstimated = recs.some((r) => r.odds_is_estimated);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-baseline gap-x-4">
        <h1 className="text-[24px] font-black">買い目の内訳</h1>
        <span className="text-ink-muted">
          {info.venue}
          {info.race_num} {info.race_name}
        </span>
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_320px] items-start gap-6 max-[1180px]:grid-cols-1">
        <section className="min-w-0">
          <div className="max-md:relative max-md:-mx-4 max-md:overflow-x-auto max-md:px-4">
          <table className="w-full border-collapse whitespace-nowrap max-md:text-[13px]">
            <thead>
              <tr className="border-b border-ink text-[11px] text-ink-muted">
                <th className="py-2 pr-3 text-left font-medium">券種</th>
                <th className="py-2 pr-3 text-left font-medium">組み合わせ</th>
                <th className="py-2 pr-3 text-left font-medium">馬名</th>
                <th className="py-2 pr-3 text-right font-medium">的中率</th>
                <th className="py-2 pr-3 text-right font-medium">オッズ</th>
                <th className="py-2 pr-3 text-right font-medium">期待値</th>
                <th className="py-2 pr-3 text-right font-medium">金額</th>
                {info.is_finished && <th className="py-2 text-right font-medium">結果</th>}
              </tr>
            </thead>
            <tbody>
              {recs.map((r, i) => (
                <tr key={i} className={`border-b border-line-soft ${r.is_hit ? "bg-hit/5" : ""}`}>
                  <td className="py-2 pr-3 font-bold whitespace-nowrap">{r.bet_type}</td>
                  <td className="py-2 pr-3">
                    <span className="flex gap-0.5">
                      {r.horses.map((h) => (
                        <Cloth key={h} waku={wakuOf(h)} umaban={h} size="sm" />
                      ))}
                    </span>
                  </td>
                  <td className="py-2 pr-3">
                    <div>{r.horse_names.join(" ・ ")}</div>
                    <div className="text-[11px] text-ink-muted">{r.confidence}</div>
                  </td>
                  <td className="py-2 pr-3 text-right num text-[15px]">{pct(r.est_hit_prob)}</td>
                  <td className="py-2 pr-3 text-right">
                    <Odds value={r.est_odds} estimated={r.odds_is_estimated} />
                  </td>
                  <td className="py-2 pr-3 text-right">
                    <span className={`num text-[16px] ${r.expected_value >= 1.2 ? "font-bold" : "text-ink-muted"}`}>
                      {r.expected_value.toFixed(2)}
                    </span>
                  </td>
                  <td className="py-2 pr-3 text-right num text-[17px] font-bold">{r.suggested_amount.toLocaleString()}</td>
                  {info.is_finished && (
                    <td className="py-2 text-right whitespace-nowrap">
                      {r.is_hit ? (
                        <span className="font-bold text-hit">
                          的中 <span className="num text-[16px]">{yen(r.actual_return ?? 0)}</span>
                        </span>
                      ) : (
                        <span className="text-ink-faint">不的中</span>
                      )}
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <td colSpan={6} className="pt-3 text-right font-bold">
                  合計
                </td>
                <td className="pt-3 pr-3 text-right num text-[20px] font-bold">{yen(result.total_bet)}</td>
                {info.is_finished && (
                  <td className="pt-3 text-right num text-[18px] font-bold">
                    {yen(result.total_return ?? 0)}
                    <span className="ml-1 text-[13px] text-ink-muted">{result.roi_percent?.toFixed(1)}%</span>
                  </td>
                )}
              </tr>
            </tfoot>
          </table>
          </div>
          <p className="mt-4 max-w-[72ch] text-[12px] leading-relaxed text-ink-muted">
            的中率は、過去走・スピード指数・騎手・父の成績から当てはめた勝率をもとに、2・3着の偏りを補正した Harville モデルで計算しています。
            2023〜2025年のJRA 1518レースでの検証では、勝率の当たり具合は単勝オッズ（市場）の方が上でした。期待値が1を超える買い目も、モデルと市場の意見の差であって、勝てる保証ではありません。
            {anyEstimated &&
              "「推定」のオッズは実オッズが取れなかった券種で、単勝オッズから同じモデルで推定した値です。期待値の判断には実オッズを使ってください。"}
          </p>
        </section>

        <aside className="rounded-lg border border-line bg-paper">
          <h2 className="border-b border-line px-4 py-2.5 text-[14px] font-black">確定払戻</h2>
          {result.official_payouts.length === 0 ? (
            <p className="px-4 py-6 text-[12px] text-ink-muted">
              レースが確定すると、ここに払戻金が表示されます。保存した予想は「回収率」画面の「結果照合」で採点できます。
            </p>
          ) : (
            <ul>
              {result.official_payouts.map((p, i) => (
                <li key={i} className="grid grid-cols-[52px_1fr_auto] items-center gap-2 border-b border-line-soft px-4 py-1.5">
                  <span className="text-[11px] font-bold text-ink-muted">{p.bet_type}</span>
                  <span className="num text-[15px]">{p.combination_text}</span>
                  <span className="text-right">
                    <span className="num text-[16px] font-bold">{yen(p.payout_yen)}</span>
                    {p.popularity && <span className="ml-1 text-[10px] text-ink-faint">{p.popularity}人気</span>}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </aside>
      </div>
    </div>
  );
}
