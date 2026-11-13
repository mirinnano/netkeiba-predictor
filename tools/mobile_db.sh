#!/bin/sh
# スマホ用の keiba.db を作り、接続した Android 端末のアプリ（デバッグ版）に入れる。
#   tools/mobile_db.sh <元の keiba.db> [--push]
# 調教は、アプリが使う分（基準年 2011・2021 年と、直近 180 日）だけ残す。レースと出走は全部残す（2011 年からの履歴を使うため）。
set -e
SRC="$1"
OUT="$(dirname "$SRC")/keiba.mobile.db"
CUT=$(date -v-180d +%Y%m%d)
rm -f "$OUT"
sqlite3 "$SRC" "VACUUM INTO '$OUT'"
sqlite3 "$OUT" "DELETE FROM works WHERE NOT ((date BETWEEN 20110101 AND 20111231) OR (date BETWEEN 20210101 AND 20211231) OR date >= $CUT); VACUUM;"
sqlite3 "$OUT" "PRAGMA integrity_check;" | grep -q '^ok$'
ls -lh "$OUT"
if [ "$2" = "--push" ]; then
  ADB="${ANDROID_HOME:-$HOME/Library/Android/sdk}/platform-tools/adb"
  PKG=com.keibascope.predictor
  "$ADB" push "$OUT" /data/local/tmp/keiba.db
  # デバッグ版のアプリのデータフォルダ（app_data_dir）に置く
  # Android の app_data_dir はアプリのデータフォルダの直下（files/ ではない）
  "$ADB" shell "run-as $PKG sh -c 'cp /data/local/tmp/keiba.db ./keiba.db && ls -l keiba.db'"
  "$ADB" shell rm /data/local/tmp/keiba.db
fi
