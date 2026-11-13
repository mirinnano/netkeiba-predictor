package com.keibascope.predictor

import android.content.Intent
import android.os.Bundle
import java.io.File
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // PC から送る DB の受け取り口（Android/data/<パッケージ>/files）を、アプリの持ち物として作っておく
    getExternalFilesDir(null)
    super.onCreate(savedInstanceState)
  }

  // 後ろに回るとき、見張るレースがあれば前面サービスを始める（まだ画面が見えているうちに始める）
  override fun onPause() {
    super.onPause()
    if (File(dataDir, "watching").exists()) {
      try {
        startForegroundService(Intent(this, PayoutWatchService::class.java))
      } catch (_: Exception) {
      }
    }
  }

  // 前に戻ったら、画面側で見張れるのでサービスは止める
  override fun onResume() {
    super.onResume()
    stopService(Intent(this, PayoutWatchService::class.java))
  }
}
