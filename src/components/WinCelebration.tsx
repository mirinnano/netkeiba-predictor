import React, { useEffect, useRef, useState } from "react";
import { RacePredictionResult } from "../types";
import { Cloth } from "./Cloth";
import { myTotals } from "./MyBets";
import { ParticleKind, raceTheme } from "../lib/raceThemes";

/** 決まった並びの乱数（描画のたびに位置が変わらないように） */
const rnd = (i: number, k: number) => {
  const x = Math.sin(i * 12.9898 + k * 78.233) * 43758.5453;
  return x - Math.floor(x);
};

/** G1 のタイトルごとに降らせるもの。花びら・葉・雪・星・紙吹雪・砂・花火 */
function Particles({ kind, colors, startMs }: { kind: ParticleKind; colors: string[]; startMs: number }) {
  if (kind === "spark") {
    // 花火: 3か所で放射状に開く
    const bursts = [
      { x: 25, y: 22, d: 0 },
      { x: 72, y: 30, d: 350 },
      { x: 45, y: 62, d: 700 },
    ];
    return (
      <div className="pointer-events-none absolute inset-0" aria-hidden>
        {bursts.flatMap((b, bi) =>
          Array.from({ length: 14 }, (_, i) => {
            const a = (i / 14) * Math.PI * 2;
            const r = 70 + rnd(i, bi) * 40;
            return (
              <span
                key={`${bi}-${i}`}
                className="particle-spark absolute h-1.5 w-1.5 rounded-full"
                style={
                  {
                    left: `${b.x}%`,
                    top: `${b.y}%`,
                    background: colors[(i + bi) % colors.length],
                    "--dx": `${Math.cos(a) * r}px`,
                    "--dy": `${Math.sin(a) * r}px`,
                    animationDelay: `${startMs + b.d}ms`,
                  } as React.CSSProperties
                }
              />
            );
          }),
        )}
      </div>
    );
  }
  const n = kind === "sand" ? 40 : kind === "snow" ? 36 : 28;
  return (
    <div className="pointer-events-none absolute inset-0" aria-hidden>
      {Array.from({ length: n }, (_, i) => (
        <span
          key={i}
          className={`particle particle-${kind} absolute`}
          style={
            {
              left: kind === "sand" ? "-4%" : `${rnd(i, 1) * 100}%`,
              top: kind === "sand" ? `${20 + rnd(i, 1) * 70}%` : undefined,
              background: colors[i % colors.length],
              "--drift": `${(rnd(i, 2) - 0.5) * 160}px`,
              "--rot": `${(rnd(i, 3) - 0.5) * 900}deg`,
              animationDuration: `${(kind === "snow" ? 2600 : kind === "sand" ? 1400 : 1900) + rnd(i, 4) * 900}ms`,
              animationDelay: `${startMs + rnd(i, 5) * 900}ms`,
            } as React.CSSProperties
          }
        />
      ))}
    </div>
  );
}

const WAKU_COLORS = ["#FFFFFF", "#1A1A1A", "#E0262B", "#1F5FBF", "#F2C200", "#1F8A3B", "#F07C1A", "#F06FA4"];

/** 重賞の格。1=GI, 2=GII, 3=GIII, 0=それ以外 */
export function gradeTier(grade: string | null | undefined): 0 | 1 | 2 | 3 {
  if (!grade) return 0;
  if (/^(GI|G1|JpnI)$/.test(grade)) return 1;
  if (/^(GII|G2|JpnII)$/.test(grade)) return 2;
  if (/^(GIII|G3|JpnIII)$/.test(grade)) return 3;
  return 0;
}

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
 * 的中したときの演出。格ごとに変える。外れたときは出さない。
 * - GI: 発馬機の扉が開く → ゴール板を線が駆け抜ける → 着順 → 払戻額が駆け上がる
 * - GII: 枠の8色のテープが降る → 着順 → 払戻額
 * - GIII: 写真判定のフラッシュ → 着順が打ち込まれる → 払戻額
 * - それ以外: 払戻額のカウントアップだけ
 */
export function WinCelebration({ result, onClose }: { result: RacePredictionResult; onClose: () => void }) {
  const tier = gradeTier(result.race_info.grade);
  const big = tier === 1;
  const theme = raceTheme(result.race_info);
  const mine = myTotals(result);
  const hits = mine.bets.filter((r) => r.is_hit);
  const total = mine.ret;
  const top3 = [...result.predictions]
    .filter((p) => p.entry.actual_rank && p.entry.actual_rank <= 3)
    .sort((a, b) => (a.entry.actual_rank ?? 9) - (b.entry.actual_rank ?? 9));
  const wakuOf = (u: number) => result.predictions.find((p) => p.entry.umaban === u)?.entry.waku ?? 1;

  // G1 の段取り: 0.45s ゲート表示 → 0.65s で開く → 線が抜ける → 着順 → 払戻
  const t =
    tier === 1
      ? { gate: 450, ranks: 1600, money: 1900, count: 1400 }
      : tier === 2
        ? { gate: 0, ranks: 900, money: 1200, count: 1200 }
        : tier === 3
          ? { gate: 0, ranks: 450, money: 800, count: 1100 }
          : { gate: 0, ranks: 0, money: 150, count: 900 };
  const amount = useCountUp(total, t.money, t.count);
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
      navigator.vibrate?.(tier === 1 ? [60, 80, 60, 80, 220] : tier > 0 ? [60, 80, 160] : [80]);
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
      {/* G1: タイトルごとのモチーフ。決まっていなければゴール板を線が駆け抜ける */}
      {big && theme.kind !== "sweep" && <Particles kind={theme.kind} colors={theme.colors} startMs={900} />}
      {big && theme.kind === "sweep" && (
        <div className="pointer-events-none absolute inset-0" aria-hidden>
          <div className="finish-post absolute bottom-0 right-[18%] top-0 w-1.5 bg-white/80" />
          <div className="finish-sweep absolute inset-y-[38%] left-0 flex w-[60%] flex-col justify-center gap-1">
            {WAKU_COLORS.map((c) => (
              <span key={c} className="block h-1.5 rounded-full" style={{ background: c }} />
            ))}
          </div>
        </div>
      )}

      {/* GII: 枠の8色のテープが降る */}
      {tier === 2 && (
        <div className="pointer-events-none absolute inset-0" aria-hidden>
          {Array.from({ length: 16 }, (_, i) => (
            <span
              key={i}
              className="ribbon absolute top-0 block w-2 rounded-full"
              style={{
                left: `${(i * 61) % 100}%`,
                height: `${18 + ((i * 37) % 22)}vh`,
                background: WAKU_COLORS[i % 8],
                animationDelay: `${(i % 8) * 60 + Math.floor(i / 8) * 180}ms`,
                animationDuration: `${1300 + ((i * 53) % 500)}ms`,
              }}
            />
          ))}
        </div>
      )}

      {/* GIII: 写真判定のフラッシュと決勝線 */}
      {tier === 3 && (
        <div className="pointer-events-none absolute inset-0" aria-hidden>
          <div className="photo-line absolute bottom-0 left-1/2 top-0 w-0 border-l-2 border-dashed border-white/70" />
          <div className="photo-flash absolute inset-0 bg-white" />
        </div>
      )}

      <div className="relative w-full max-w-sm text-center text-white">
        <div className="text-[13px] font-bold tracking-[0.2em] text-waku-5" style={big ? { color: theme.accent } : undefined}>
          {result.race_info.grade && tier > 0 ? `${result.race_info.grade} ・ ` : ""}
          {result.race_info.race_name}
        </div>
        <div className="mt-1 text-[44px] font-black leading-none">的中</div>

        <div className="mt-5 flex justify-center gap-3">
          {top3.map((p, i) => (
            <div
              key={p.entry.umaban}
              className={`celebrate-rank ${tier === 3 ? "is-stamp" : ""} flex flex-col items-center gap-1 ${phase >= 2 ? "is-in" : ""}`}
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
          {mine.bet.toLocaleString()}円 → 回収率 {mine.bet > 0 ? Math.round((total / mine.bet) * 100) : 0}%
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
