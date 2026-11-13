import { ReactNode, useRef } from "react";

/** value が前回の描画から変わったときだけ、中身の背景を一瞬光らせる。最初の表示では光らせない */
export function Flash({ value, children, className = "" }: { value: unknown; children: ReactNode; className?: string }) {
  const prev = useRef(value);
  const count = useRef(0);
  if (prev.current !== value) {
    prev.current = value;
    count.current += 1;
  }
  return (
    <span key={count.current} className={`${count.current > 0 ? "value-flash" : ""} ${className}`}>
      {children}
    </span>
  );
}
