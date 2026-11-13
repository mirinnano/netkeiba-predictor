import { useEffect, useRef, useState } from "react";
import { RacePredictionResult } from "../types";
import { Cloth } from "./Cloth";

const WAKU_COLORS = ["#FFFFFF", "#1A1A1A", "#E0262B", "#1F5FBF", "#F2C200", "#1F8A3B", "#F07C1A", "#F06FA4"];

export const isG1 = (grade: string | null | undefined) => !!grade && /^(GI|G1|JpnI)$/.test(grade);

/** 0 から target まで、ease-out で数字を上げる */
function useCountUp(target: number, startAfterMs: number, durationMs: number) {
  const [v, setV] = useState(0);
  useEffect(() => {
    let raf = 0;
    const t0 = performance.now() + startAfterMs;
    const tick = (t: number) => {
      const k = Math.min(1, Math.max(0, (t - t0) / durationMs));
      setV(Math.round(target * (1 - Math.pow(1 - k, 4))));
      if (k < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [target, startAfterMs, durationMs]);
  return v;
}

/**
 * 的中したときの演出。G1 はゲートが開く → ゴール板を駆け抜ける → 払戻額が駆け上がる。
 * G1 以外は払戻額のカウントアップだけ。外れたときは出さない。
 */
export function WinCelebration({ result, onClose }: { result: RacePredictionResult; onClose: () => void }) {
  const big = isG1(result.race_info.grade);
  const hits = result.recommendations.filter((r) => r.is_hit);
  const total = result.total_return ?? 0;
  const top3 = [...result.predictions]
    .filter((p) => p.entry.actual_rank && p.entry.actual_rank <= 3)
    .sort((a, b) => (a.entry.actual_rank ?? 9) - (b.entry.actual_rank ?? 9));
  const wakuOf = (u: number) => result.predictions.find((p) => p.entry.umaban === u)?.entry.waku ?? 1;

  // G1 の段取り: 0.45s ゲート表示 → 0.65s で開く → 線が抜ける → 着順 → 払戻
  const t = big ? { gate: 450, sweep: 1000, ranks: 1600, money: 1900 } : { gate: 0, sweep: 0, ranks: 0, money: 150 };
  const amount = useCountUp(total, t.money, big ? 1400 : 900);
  const [phase, setPhase] = useState(0);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const ids = [
      window.setTimeout(() => setPhase(1), t.gate),
      window.setTimeout(() => setPhase(2), t.ranks),
      window.setTimeout(() => setPhase(3), t.money),
    ];
    // 端末を短く震わせる（対応していなければ何もしない）
    try {
      navigator.vibrate?.(big ? [60, 80, 60, 80, 220] : [80, 60, 140]);
    } catch {
      /* noop */
    }
    closeRef.current?.focus();
    return () => ids.forEach(window.clearTimeout);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`的中 ${total.toLocaleString()}円`}
      className="celebrate-backdrop fixed inset-0 z-50 flex items-center justify-center overflow-hidden bg-ink/95 px-5"
      onClick={() => phase >= 3 && onClose()}
    >
      {/* ゴール板を駆け抜ける線（G1 のみ） */}
      {big && (
        <div className="pointer-events-none absolute inset-0" aria-hidden>
          <div className="finish-post absolute bottom-0 right-[18%] top-0 w-1.5 bg-white/80" />
          <div className="finish-sweep absolute inset-y-[38%] left-0 flex w-[60%] flex-col justify-center gap-1">
            {WAKU_COLORS.map((c) => (
              <span key={c} className="block h-1.5 rounded-full" style={{ background: c }} />
            ))}
          </div>
        </div>
      )}

      <div className="relative w-full max-w-sm text-center text-white">
        <div className="text-[13px] font-bold tracking-[0.2em] text-waku-5">
          {result.race_info.grade && big ? `${result.race_info.grade} ・ ` : ""}
          {result.race_info.race_name}
        </div>
        <div className="mt-1 text-[44px] font-black leading-none">的中</div>

        <div className="mt-5 flex justify-center gap-3">
          {top3.map((p, i) => (
            <div
              key={p.entry.umaban}
              className={`celebrate-rank flex flex-col items-center gap-1 ${phase >= 2 ? "is-in" : ""}`}
              style={{ transitionDelay: `${i * 70}ms` }}
            >
              <Cloth waku={p.entry.waku} umaban={p.entry.umaban} size="lg" />
              <span className="text-[11px] text-shell-text">{p.entry.actual_rank}着</span>
            </div>
          ))}
        </div>

        <div className="mt-6 text-[12px] text-shell-text">払戻</div>
        <div className="num text-[56px] font-bold leading-none text-waku-5">
          {amount.toLocaleString()}
          <span className="ml-1 text-[20px] text-white">円</span>
        </div>
        <div className="mt-1 text-[12px] text-shell-text">
          {result.total_bet.toLocaleString()}円 → 回収率 {result.roi_percent?.toFixed(0)}%
        </div>

        <ul className={`celebrate-hits mt-5 space-y-1.5 ${phase >= 3 ? "is-in" : ""}`}>
          {hits.map((h) => (
            <li key={`${h.bet_type}-${h.horses.join("-")}`} className="flex items-center justify-between rounded-md bg-white/10 px-3 py-2">
              <span className="flex items-center gap-2">
                <span className="text-[12px] font-bold">{h.bet_type}</span>
                {h.horses.map((u) => (
                  <Cloth key={u} waku={wakuOf(u)} umaban={u} size="sm" />
                ))}
              </span>
              <span className="num text-[18px] font-bold">{(h.actual_return ?? 0).toLocaleString()}円</span>
            </li>
          ))}
        </ul>

        <button
          ref={closeRef}
          type="button"
          onClick={onClose}
          className="mt-6 h-11 w-full rounded-md bg-paper text-[14px] font-bold text-ink"
        >
          閉じる
        </button>
      </div>

      {/* 発馬機（G1 のみ）: 左右の扉が開く */}
      {big && (
        <div className="pointer-events-none absolute inset-0 flex" aria-hidden>
          <div className={`gate-door gate-left flex-1 ${phase >= 1 ? "is-open" : ""}`}>
            <span className="gate-label">{result.race_info.grade}</span>
          </div>
          <div className={`gate-door gate-right flex-1 ${phase >= 1 ? "is-open" : ""}`} />
        </div>
      )}
    </div>
  );
}
