---
name: KeibaScope
description: 発走前に軸・相手・買い目・金額を決めるための個人用の予想ツール
colors:
  ground: "#F7F6F2"
  paper: "#FFFFFF"
  ink: "#16181D"
  ink-soft: "#3A3E47"
  ink-muted: "#5A5F6B"
  ink-faint: "#8A8F99"
  line: "#D9D6CC"
  line-soft: "#E8E5DC"
  shell: "#1D3FA8"
  shell-deep: "#15308A"
  shell-edge: "#4C67C4"
  shell-text: "#C9D4F5"
  mark: "#C0392B"
  mine: "#7A2BD9"
  hit: "#1F7A45"
  warn: "#9A6B00"
  waku-1: "#FFFFFF"
  waku-2: "#1A1A1A"
  waku-3: "#E0262B"
  waku-4: "#1F5FBF"
  waku-5: "#F2C200"
  waku-6: "#1F8A3B"
  waku-7: "#F07C1A"
  waku-8: "#F06FA4"
typography:
  race-title:
    fontFamily: "Yu Gothic UI, Hiragino Sans, Meiryo, sans-serif"
    fontSize: "30px"
    fontWeight: 900
    lineHeight: 1
    letterSpacing: "-0.01em"
  body:
    fontFamily: "Yu Gothic UI, Hiragino Sans, Meiryo, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
  figure-lg:
    fontFamily: "Barlow Condensed, Arial Narrow, sans-serif"
    fontSize: "26px"
    fontWeight: 700
    lineHeight: 1
  figure:
    fontFamily: "Barlow Condensed, Arial Narrow, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.2
  label:
    fontFamily: "Yu Gothic UI, Hiragino Sans, Meiryo, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: 1.4
rounded:
  sm: "2px"
  md: "6px"
  lg: "8px"
spacing:
  row-y: "6px"
  cell-x: "8px"
  panel: "16px"
  page-x: "24px"
components:
  button-primary:
    backgroundColor: "{colors.shell}"
    textColor: "{colors.paper}"
    rounded: "{rounded.md}"
    padding: "8px 16px"
  button-primary-hover:
    backgroundColor: "{colors.shell-deep}"
  button-secondary:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink-soft}"
    rounded: "{rounded.md}"
    padding: "6px 12px"
  segmented-on:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    rounded: "{rounded.sm}"
  tab-on:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.shell}"
    rounded: "{rounded.md}"
    padding: "8px 14px"
  input-shell:
    backgroundColor: "{colors.shell-deep}"
    textColor: "{colors.paper}"
    rounded: "{rounded.md}"
    height: "36px"
---

# Design System: KeibaScope

## Overview

**枠帽と勝負服。** 出走馬はコースで見るときと同じ色で読む。枠帽の8色に馬番を載せた「ゼッケン」が各馬の顔で、予想順はゼッケンの列になる。それ以外の面は昼の明るい地と墨の文字だけで組み、色は意味のある場所にだけ置く。

画面は操作する道具であり、紙面やポスターの再現ではない。ナビ・ボタン・入力・表はすべて標準のコントロールで、世界観が入るのは書体・色・密度と、ゼッケン列という一つの見せ場だけ。

## Colors

### Primary
- **Silks Blue `#1D3FA8`**: ヘッダー帯と主要ボタン。画面の外枠を塗る勝負服の色。押下時は `#15308A`。

### Neutral
- **Daylight Ground `#F7F6F2`**: ページの地。昼の部屋で読む前提の、わずかに温かい白。
- **Paper `#FFFFFF`**: パネル、選択中の行、展開した詳細。
- **Ink `#16181D`** と段階 `#3A3E47` / `#5A5F6B` / `#8A8F99`: 本文、補足、ラベル、無効。
- **Line `#D9D6CC` / `#E8E5DC`**: 罫線。表の見出しの下だけ Ink の実線。

### Named Rules
- **枠色はゼッケン専用。** 8つの枠色は馬番の布にだけ使い、ボタンや見出しの装飾に使わない。
- **赤インクは印だけ。** `#C0392B` は ◎○▲☆△ と危険操作（削除）に限る。
- **紫は「自分の買い目」だけ。** `#7A2BD9` は推奨買い目に入っている馬（ゼッケン列の輪、表の点）以外に使わない。
- **状態色は小さく。** 的中 `#1F7A45`、未確定 `#9A6B00` は文字色で示し、面を塗らない（的中行の 5% 地色を除く）。

## Typography

和文はシステムのゴシック（Yu Gothic UI / Hiragino Sans / Meiryo）。数字はすべて Barlow Condensed（縦長・太め）で、ゼッケンの番号と同じ声で読ませる。

### Hierarchy
- **レース名** 30px / 900。
- **◎の馬名** 30px / 900、◎記号 40px。画面で一番大きい。
- **主要な数字**（◎の指数・勝率・単勝、合計金額）Barlow Condensed 24–26px / 700。
- **表の数字** Barlow Condensed 15–18px。強い値（スコア80以上、期待値1.2以上）だけ太字。
- **本文** 13px、**ラベル** 11px。

### Named Rules
- **階層は大きさの差で作る。** 囲み枠や色面で強調しない。
- **数字は表形式の等幅（tabular-nums）。**

## Layout

- ヘッダー帯 56px（固定）。本文は最大 1600px、左右 24px。
- 出馬表画面: レース名の行 → ゼッケン列（全幅）→ 2カラム（左: ◎の行と出馬表、右: 買い目パネル 360px、1280px未満では 300px）。
- 1280px 未満（最小 1024px）では、表の5項目スコアと3着内率と血統行を隠し、ゼッケンを 42px に縮めて2カラムを保つ。
- 買い目パネルは上端に固定し、内部の買い目リストだけがスクロールする。合計と保存ボタンは常に見える位置に置く。

## Elevation & Depth

ほぼ平面。影は二つだけ。
- 選択中のゼッケン: `0 1px 3px rgba(22,24,29,0.18)`
- トースト: `0 6px 20px rgba(22,24,29,0.25)`

## Shapes

角丸は小さく: ゼッケンと入力 6px 前後、パネル 8px、バッジ 2px。ゼッケンは墨の 1px の縁取りを持つ（白枠と黄枠を地から分けるため）。

## Components

### Buttons
- 主要: Silks Blue 地に白文字（「予想する」は反転して白地に青、「この予想を保存」）。
- 副: 白地・罫線・墨文字。ホバーで縁が墨になる。
- 危険: 削除は2段階。1回目でその行に「削除する／やめる」を出す。

### Navigation
ヘッダー帯の中の4つのタブ（出馬表 / 開催一覧 / 買い目 / 回収率）。選択中は白地に青文字。予想がない間は出馬表と買い目を無効にする。

### Segmented control
配分の方針と予算。選択中は墨地に白文字。

### Inputs
ヘッダーの検索は濃紺地に白文字、数字は Barlow Condensed。

### ゼッケン列（Signature）
予想順に全頭のゼッケンを並べ、上に印、下に指数。推奨買い目の馬は紫の輪。押すと表の該当行を選択して展開する。重みを変えて順位が動くと、各ゼッケンが元の位置から新しい位置へ 420ms で滑る（FLIP、`cubic-bezier(0.16, 1, 0.3, 1)`）。動きを減らす設定では即座に入れ替わる。

### 推定オッズ
実オッズが取れない券種は、数字の前に破線枠の「推定」を付け、数字を薄くする。実オッズと同じ描き方をしない。

## Do's and Don'ts

### Do:
- 印（◎○▲☆△）と枠色は競馬の慣用どおりに使う。
- 確定後に作った予想には「確定後の予想・集計外」を必ず出す。
- 数字は Barlow Condensed で、桁を揃える。

### Don't:
- 枠色・赤・紫を装飾に使わない。
- カードの中にカードを入れない。パネルは買い目と重み調整だけ。
- グラデーション、ガラス、発光、見出しの上の小さなラベル（キッカー）を使わない。
- 推定値を実値と同じ見た目で出さない。
