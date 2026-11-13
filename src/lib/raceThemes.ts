import { RaceInfo } from "../types";

/** 的中演出で降らせるもの */
export type ParticleKind = "petal" | "leaf" | "snow" | "star" | "confetti" | "sand" | "spark" | "sweep";

export type RaceTheme = { kind: ParticleKind; colors: string[]; accent: string };

const WAKU = ["#FFFFFF", "#1A1A1A", "#E0262B", "#1F5FBF", "#F2C200", "#1F8A3B", "#F07C1A", "#F06FA4"];

const SAKURA: RaceTheme = { kind: "petal", colors: ["#FADADD", "#F7B6C8", "#FFFFFF"], accent: "#F7B6C8" };
const FRESH: RaceTheme = { kind: "leaf", colors: ["#3FA34D", "#7CC86E", "#C8E6A0"], accent: "#7CC86E" };
const MAPLE: RaceTheme = { kind: "leaf", colors: ["#C0392B", "#E07B39", "#F2C200"], accent: "#E07B39" };
const STARS: RaceTheme = { kind: "star", colors: ["#F2C200", "#FFFFFF", "#C9D4F5"], accent: "#F2C200" };
const SAND: RaceTheme = { kind: "sand", colors: ["#C9A46A", "#E3CFA3", "#8A6A3B"], accent: "#E3CFA3" };
const SWEEP: RaceTheme = { kind: "sweep", colors: WAKU, accent: "#F2C200" };

/**
 * G1 のタイトルごとの演出。開催の季節やレースの性格から選んだ、このアプリ独自のモチーフ。
 * 公式の映像・ロゴ・キャッチコピーは使わない。
 */
const TITLES: { match: RegExp; theme: RaceTheme | ((info: RaceInfo) => RaceTheme) }[] = [
  { match: /有馬記念/, theme: { kind: "snow", colors: ["#FFFFFF", "#E8EEF9", "#F2C200"], accent: "#F2C200" } },
  { match: /ホープフル|朝日杯|阪神ジュベナイル|阪神JF/, theme: STARS },
  { match: /ダービー|東京優駿/, theme: FRESH },
  { match: /オークス|優駿牝馬/, theme: { kind: "petal", colors: ["#F7C6D9", "#FFFFFF", "#E88FB0"], accent: "#E88FB0" } },
  { match: /桜花賞|大阪杯/, theme: SAKURA },
  { match: /皐月賞/, theme: { kind: "petal", colors: ["#D6336C", "#F06FA4", "#FFFFFF"], accent: "#F06FA4" } },
  { match: /菊花賞/, theme: { kind: "petal", colors: ["#F2C200", "#F7D94C", "#FFFFFF"], accent: "#F7D94C" } },
  { match: /秋華賞|マイルCS|マイルチャンピオン/, theme: MAPLE },
  { match: /天皇賞/, theme: (info) => (monthOf(info) >= 9 ? MAPLE : FRESH) },
  { match: /ジャパンC|ジャパンカップ/, theme: { kind: "confetti", colors: WAKU, accent: "#F2C200" } },
  { match: /エリザベス女王杯|ヴィクトリアマイル/, theme: { kind: "petal", colors: ["#B0306A", "#E06C9F", "#F2C200"], accent: "#E06C9F" } },
  { match: /宝塚記念/, theme: { kind: "spark", colors: ["#F2C200", "#F07C1A", "#FFFFFF", "#E0262B"], accent: "#F07C1A" } },
  { match: /安田記念/, theme: { kind: "petal", colors: ["#5B7FD6", "#8E7CC3", "#BFD0F5"], accent: "#8EA6E8" } },
  { match: /NHKマイル/, theme: FRESH },
  { match: /高松宮記念/, theme: { kind: "petal", colors: ["#F9D5E5", "#FFFFFF", "#D6336C"], accent: "#F06FA4" } },
  { match: /スプリンターズ|大障害|グランドジャンプ/, theme: SWEEP },
  { match: /チャンピオンズC|フェブラリー/, theme: SAND },
];

function monthOf(info: RaceInfo): number {
  const m = info.date_str.match(/\d{4}\D+(\d{1,2})/);
  return m ? +m[1] : 0;
}

export function raceTheme(info: RaceInfo): RaceTheme {
  for (const t of TITLES) {
    if (t.match.test(info.race_name)) return typeof t.theme === "function" ? t.theme(info) : t.theme;
  }
  // 名前で決まらない G1（地方の JpnI など）は、ダートなら砂、芝なら線が駆け抜ける
  return info.surface.startsWith("ダ") ? SAND : SWEEP;
}
