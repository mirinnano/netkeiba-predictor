import { clothClass } from "../lib/format";

type Size = "xs" | "sm" | "md" | "lg" | "xl";

const SIZE: Record<Size, string> = {
  xs: "w-5 h-5 text-[13px]",
  sm: "w-6 h-6 text-[15px]",
  md: "w-8 h-8 text-lg",
  lg: "w-11 h-11 text-[26px]",
  xl: "w-[54px] h-[48px] text-[28px] max-[1280px]:w-[42px] max-[1280px]:h-[40px] max-[1280px]:text-[22px]",
};

/** ゼッケン。枠色の布に馬番 */
export function Cloth({
  waku,
  umaban,
  size = "sm",
  className = "",
}: {
  waku: number;
  umaban: number;
  size?: Size;
  className?: string;
}) {
  return (
    <span className={`cloth ${SIZE[size]} ${clothClass(waku)} ${className}`} aria-label={`${umaban}番`}>
      {umaban}
    </span>
  );
}
