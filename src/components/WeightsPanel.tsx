import { FactorShare, ScoringWeights } from "../types";

/** 既定の重み（API との互換のために送るだけ。勝率の計算には使わない） */
export const DEFAULT_WEIGHTS: ScoringWeights = { ability: 30, speed: 25, aptitude: 20, jockey_frame: 15, pace: 10 };

/**
 * 指数の内訳。重みは手で決めず、モデルの係数から自動で決まる。
 * 割合は「このレースで、その要素が馬どうしの差をどれだけ作っているか」。
 */
export function FactorShares({ shares, model }: { shares: FactorShare[]; model?: string }) {
  if (shares.length === 0) {
    return (
      <div className="rounded-lg border border-line bg-paper px-4 py-3 text-[12px] text-ink-muted">
        この予想には内訳がありません（保存された古い予想です）。取り直すと表示されます。
      </div>
    );
  }
  const max = Math.max(...shares.map((s) => s.share));
  return (
    <div className="rounded-lg border border-line bg-paper px-4 py-3">
      <div className="mb-2 flex flex-wrap items-baseline gap-x-3 text-[12px]">
        <span className="font-bold">指数の内訳（自動）</span>

      </div>
      <ul className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1.5">
        {shares.map((s) => (
          <li key={s.name} className="contents">
            <span className="text-[12px] font-bold">{s.name}</span>
            <span className="h-2 rounded-sm bg-line-soft">
              <span className="block h-2 rounded-sm bg-shell" style={{ width: `${(s.share / max) * 100}%` }} />
            </span>
            <span className="num w-10 text-right text-[15px] text-ink-muted">{Math.round(s.share * 100)}%</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
