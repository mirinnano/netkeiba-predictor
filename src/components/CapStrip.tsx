import { useLayoutEffect, useRef } from "react";
import { HorsePrediction } from "../types";
import { Cloth } from "./Cloth";

/**
 * 予想順に並べたゼッケンの列。重みを変えて順位が動くと、各ゼッケンが元の位置から新しい位置へ滑る（FLIP）。
 * ゼッケンを押すと表の該当行に移る。
 */
export function CapStrip({
  ordered,
  tickets,
  selected,
  onSelect,
}: {
  ordered: HorsePrediction[];
  tickets: Set<number>;
  selected: number | null;
  onSelect: (umaban: number) => void;
}) {
  const refs = useRef(new Map<number, HTMLButtonElement>());
  const prev = useRef(new Map<number, number>());

  useLayoutEffect(() => {
    const next = new Map<number, number>();
    refs.current.forEach((el, key) => next.set(key, el.getBoundingClientRect().left));
    next.forEach((left, key) => {
      const before = prev.current.get(key);
      const el = refs.current.get(key);
      if (before === undefined || !el || before === left) return;
      el.animate(
        [{ transform: `translateX(${before - left}px)` }, { transform: "translateX(0)" }],
        { duration: 420, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
      );
    });
    prev.current = next;
  });

  return (
    <div className="flex gap-1.5 overflow-x-auto pb-1" role="list" aria-label="予想順">
      {ordered.map((p) => {
        const e = p.entry;
        const mine = tickets.has(e.umaban);
        const isSel = selected === e.umaban;
        return (
          <button
            key={e.umaban}
            ref={(el) => {
              if (el) refs.current.set(e.umaban, el);
              else refs.current.delete(e.umaban);
            }}
            type="button"
            role="listitem"
            onClick={() => onSelect(e.umaban)}
            title={`${p.mark || ""} ${e.umaban} ${e.horse_name}  指数${p.total_index.toFixed(1)}`}
            className={`group shrink-0 w-[62px] max-[1280px]:w-[50px] flex flex-col items-center rounded-md pt-0.5 pb-1 transition-colors ${
              isSel ? "bg-paper shadow-[0_1px_3px_rgba(22,24,29,0.18)]" : "hover:bg-paper/70"
            } ${e.is_scratched ? "opacity-35" : ""}`}
          >
            <span className="h-6 text-[20px] font-black text-mark leading-6">{p.mark === "消" ? "" : p.mark}</span>
            <span className={`rounded-[7px] p-[2px] ${mine ? "ring-2 ring-mine" : "ring-0"}`}>
              <Cloth waku={e.waku} umaban={e.umaban} size="xl" />
            </span>
            <span className="num mt-1 text-[13px] font-semibold text-ink-muted group-hover:text-ink">
              {e.is_scratched ? "取消" : p.total_index.toFixed(1)}
            </span>
          </button>
        );
      })}
    </div>
  );
}
