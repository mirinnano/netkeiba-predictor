# JvDump: JRA-VAN のデータを取り出す（Windows 用）

JV-Link から JV-Data のレコードを取り出し、**加工せずに** Shift_JIS のテキストとして保存します。
各項目の位置の解釈は Mac 側で、JV-Data 仕様書に従って行います。

JV-Link は Wine では初期化（JVInit）で落ちるため、Windows PC で動かします（2026-11-12 確認）。

## 必要なもの

- Windows PC（JV-Link をインストール済み）
- JRA-VAN Data Lab の利用キー
- .NET Framework 4.x（Windows 10/11 には標準で入っています）

追加のダウンロードはありません。ビルドには Windows 付属の C# コンパイラを使います。

## 手順

1. このフォルダ（`tools/jvlink-dump`）を Windows PC にコピーする。
2. コマンドプロンプトでこのフォルダに移動し、ビルドする。

   ```
   build.bat
   ```

   `Built JvDump.exe` と出れば成功です。

3. 利用キーを設定する（初回だけ）。JV-Link の設定画面が開くので、利用キーを入力します。

   ```
   JvDump.exe setup
   ```

4. データを取り出す。既定では 2021年1月1日以降の RACE・SLOP・WOOD・DIFN・BLOD を `out` に保存します。

   ```
   dump_all.bat
   ```

   開始日を変えるときは `dump_all.bat 20230101000000` のように指定します。
   初回は大量のダウンロードになり、数時間かかることがあります。

5. `out` フォルダを Mac に渡す（USB・クラウドなど）。Mac 側では `backtest/jvdata/` に置いてください。

## 出力

```
out/
  RACE/
    RA.txt   レース詳細
    SE.txt   馬毎レース情報
    HR.txt   払戻
    O1.txt … 確定オッズ（券種ごと）
    lastfiletimestamp.txt   次回の差分取得に使う時刻
  SLOP/HC.txt   坂路調教
  WOOD/WC.txt   ウッドチップ調教
  ...
```

ファイル名は各レコードの先頭2文字（レコード種別）です。1行が1レコードで、位置はバイト単位で決まっています。

## 個別に取り出す

```
JvDump.exe open <データ種別> <開始日時 yyyyMMddHHmmss> <オプション> <出力先>
JvDump.exe rt <データ種別> <キー> <出力先>
```

## 確かめていないこと（最初の実行で確認する）

この道具は Windows で動かしたことがありません。次の点は JRA-VAN SDK の仕様書で確認し、違えば直します。

- `JVOpen` のオプションの番号（ここでは 4 を「ダイアログなしのセットアップ」として使っています）
- データ種別の名前と、それぞれに含まれるレコード種別（RACE・SLOP・WOOD・DIFN・BLOD）
- 時系列オッズ（`rt` の `0B41` など）を過去何年分取れるか、キーの形式
- 坂路・ウッドチップ調教のデータがいつから提供されているか

エラーで止まったら、表示された数字（`JVOpen=-111` など）をそのまま教えてください。JV-Link の戻り値の意味から原因を調べます。
