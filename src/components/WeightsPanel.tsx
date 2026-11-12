import { ScoringWeights } from "../types";

export const WEIGHT_ITEMS: { key: keyof ScoringWeights; label: string }[] = [
  { key: "ability", label: "能力・着順" },
  { key: "speed", label: "時計・上がり" },
  { key: "aptitude", label: "コース適性" },
  { key: "jockey_frame", label: "騎手・枠" },
  { key: "pace", label: "展開" },
];

export const PRESETS: { name: string; w: ScoringWeights }[] = [
  { name: "標準", w: { ability: 30, speed: 25, aptitude: 20, jockey_frame: 15, pace: 10 } },
  { name: "時計・末脚", w: { ability: 20, speed: 45, aptitude: 15, jockey_frame: 10, pace: 10 } },
  { name: "適性・騎手", w: { ability: 20, speed: 15, aptitude: 35, jockey_frame: 25, pace: 5 } },
  { name: "展開", w: { ability: 20, speed: 20, aptitude: 15, jockey_frame: 15, pace: 30 } },
];

export function WeightsPanel({
  weights,
  onChange,
  onPreset,
}: {
  weights: ScoringWeights;
  onChange: (w: ScoringWeights) => void;
  onPreset: (w: ScoringWeights) => void;
}) {
  const total = WEIGHT_ITEMS.reduce((s, i) => s + weights[i.key], 0) || 1;
  return (
    <div className="flex flex-wrap items-end gap-x-6 gap-y-3 rounded-lg border border-line bg-paper px-4 py-3">
      {WEIGHT_ITEMS.map((item) => (
        <label key={item.key} className="flex w-[150px] flex-col gap-1">
          <span className="flex items-baseline justify-between text-[12px]">
            <span className="font-bold">{item.label}</span>
            <span className="num text-[15px] text-ink-muted">{Math.round((weights[item.key] / total) * 100)}%</span>
          </span>
          <input
            type="range"
            min={0}
            max={60}
            step={5}
            value={weights[item.key]}
            onChange={(e) => onChange({ ...weights, [item.key]: Number(e.target.value) })}
            className="accent-shell"
          />
        </label>
      ))}
      <div className="ml-auto flex items-center gap-1.5">
        <span className="text-[11px] text-ink-muted">プリセット</span>
        {PRESETS.map((p) => (
          <button
            key={p.name}
            type="button"
            onClick={() => onPreset(p.w)}
            className="rounded border border-line px-2.5 py-1 text-[12px] font-bold text-ink-soft hover:border-ink hover:text-ink"
          >
            {p.name}
          </button>
        ))}
      </div>
    </div>
  );
}
