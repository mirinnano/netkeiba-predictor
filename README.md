# KeibaScope — netkeiba JRA/NAR 競馬予測・回収率シミュレーション (Tauri v2)

`netkeiba.com`（中央競馬 JRA / 地方競馬 NAR）から出馬表・過去5走ラップ・リアルタイムオッズ・確定払戻をスクレイピングし、独自の5大ファクター総合指数とHarvilleモデルによる期待値（EV）・回収率シミュレーションを行うTauri v2デスクトップアプリケーションです。

## 主な機能

- **中央競馬 (JRA) ＆ 地方競馬 (NAR) 完全対応**
  - JRA (`race.netkeiba.com`) と地方競馬 (`nar.netkeiba.com`：大井・川崎・船橋・浦和・門別・盛岡・水沢・金沢・笠松・名古屋・園田・姫路・高知・佐賀・帯広) の両方に対応
  - 12桁の `race_id` または netkeiba のURLから自動判別してスクレイピング＆予測
- **5大ファクター・スコアリングエンジン (Rust)**
  1. **過去着順・安定感 (`past_finish`)**: 直近5走の着順・着差・クラス実績
  2. **スピード・上がり3F (`speed_3f`)**: 上がり3F順位・走破時計比較
  3. **コース・距離・馬場適性 (`course_aptitude`)**: 芝/ダート・距離・開催場の一致度
  4. **騎手・枠順・斤量 (`jockey_frame`)**: JRA/NAR主要騎手レート・枠順バイアス・斤量増減
  5. **脚質・展開バイアス (`pace_style`)**: 過去通過順からの脚質分類とレース全体のペース予測
- **期待値（EV）＆ 予算配分シミュレーター**
  - Harvilleモデルによる勝率・連対率・複勝圏確率の推定
  - 単勝・馬連・ワイド・3連複の推奨買い目と合成オッズ・トリガミ判定
  - 確定レース (`result.html`) の公式払戻データとの自動照合・収支計算
- **開催レースブラウザ**
  - 中央 (JRA) / 地方 (NAR) 切替対応の開催日カレンダー＆会場別1R〜12R一覧
- **SQLite ローカルDB保存 ＆ 回収率ダッシュボード**
  - 予想スナップショット・メモのローカル永続化と回収率（ROI）・的中率の集計

## 技術スタック

- **Backend**: Rust / Tauri v2 (`reqwest`, `scraper`, `encoding_rs`, `rusqlite`, `tokio`)
- **Frontend**: React 19 + TypeScript + Tailwind CSS + Lucide Icons + Vite

## 起動・ビルド方法

```powershell
# 依存パッケージのインストール
npm install

# フロントエンドビルド + スタンドアロン実行バイナリの作成
npm run build
cd src-tauri
cargo build
```

ビルド後、`start-app.bat` または `src-tauri\target\debug\netkeiba-predictor.exe` から単体起動できます。
