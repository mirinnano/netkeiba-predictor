package com.keibascope.predictor

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // PC から送る DB の受け取り口（Android/data/<パッケージ>/files）を、アプリの持ち物として作っておく
    getExternalFilesDir(null)
    super.onCreate(savedInstanceState)
  }
}
