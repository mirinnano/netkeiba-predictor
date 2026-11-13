// 馬券の組み合わせを作る。ボックス・流し・フォーメーション
// 返す配列の各要素は馬番の並び。順序のない券種（馬連・ワイド・3連複）は昇順、順序のある券種（馬単・3連単）は着順どおり。

export type BetKind = { type: string; n: number; ordered: boolean };

export const BET_KINDS: BetKind[] = [
  { type: "単勝", n: 1, ordered: false },
  { type: "複勝", n: 1, ordered: false },
  { type: "馬連", n: 2, ordered: false },
  { type: "ワイド", n: 2, ordered: false },
  { type: "馬単", n: 2, ordered: true },
  { type: "3連複", n: 3, ordered: false },
  { type: "3連単", n: 3, ordered: true },
];

export type Method = "single" | "box" | "wheel" | "formation";

const keyOf = (v: number[]) => v.join("-");

/** 重複と同じ馬を含む並びを除き、順序のない券種は昇順にそろえる */
function normalize(list: number[][], ordered: boolean): number[][] {
  const seen = new Set<string>();
  const out: number[][] = [];
  for (const raw of list) {
    if (new Set(raw).size !== raw.length) continue;
    const v = ordered ? raw : [...raw].sort((a, b) => a - b);
    const k = keyOf(v);
    if (!seen.has(k)) {
      seen.add(k);
      out.push(v);
    }
  }
  return out;
}

/** 各列から 1 頭ずつ選ぶ全通り（フォーメーションの元） */
function product(cols: number[][]): number[][] {
  return cols.reduce<number[][]>((acc, col) => acc.flatMap((a) => col.map((x) => [...a, x])), [[]]);
}

/** ボックス: 選んだ馬の中のすべての組み合わせ（順序のある券種は並べ替えも全部） */
export function box(kind: BetKind, horses: number[]): number[][] {
  return normalize(product(Array.from({ length: kind.n }, () => horses)), kind.ordered);
}

/**
 * 流し: 軸と相手。軸は 1 頭（3 連系は 2 頭まで）。
 * 順序のある券種では、軸を何着に置くかを axisPos（0 始まり）で決める。軸 2 頭のときは軸を 1・2 着、または指定の 2 か所。
 * multi を true にすると、順序のある券種で軸の着順を問わない（マルチ）。
 */
export function wheel(kind: BetKind, axis: number[], others: number[], axisPos: number[] = [0], multi = false): number[][] {
  if (axis.length === 0 || axis.length >= kind.n) return [];
  const rest = others.filter((h) => !axis.includes(h));
  if (!kind.ordered) {
    const cols = [...axis.map((a) => [a]), ...Array.from({ length: kind.n - axis.length }, () => rest)];
    return normalize(product(cols), false);
  }
  if (multi) {
    // 軸と相手から作った並びのうち、軸がすべて入っているもの
    return box(kind, [...axis, ...rest]).filter((v) => axis.every((a) => v.includes(a)) && v.filter((x) => !axis.includes(x)).length === kind.n - axis.length);
  }
  const cols: number[][] = Array.from({ length: kind.n }, () => rest);
  axis.forEach((a, i) => {
    const pos = axisPos[i] ?? i;
    if (pos < kind.n) cols[pos] = [a];
  });
  return normalize(product(cols), true);
}

/** フォーメーション: 列ごと（1 着・2 着・3 着、または 1 頭目・2 頭目・3 頭目）に選んだ馬の全通り */
export function formation(kind: BetKind, cols: number[][]): number[][] {
  if (cols.length !== kind.n || cols.some((c) => c.length === 0)) return [];
  return normalize(product(cols), kind.ordered);
}
