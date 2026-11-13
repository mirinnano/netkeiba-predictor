package com.keibascope.predictor

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import java.io.File

/**
 * 払戻を待つあいだ、アプリのプロセスを生かしておくための前面サービス。
 * 見張り自体はアプリ本体（Rust）が行い、見張るレースがあるあいだ dataDir/watching を置く。
 * この印が消えたら、サービスも自分で止まる。
 */
class PayoutWatchService : Service() {
  private val handler = Handler(Looper.getMainLooper())
  private val startedAt = System.currentTimeMillis()

  private val check = object : Runnable {
    override fun run() {
      val flag = File(dataDir, "watching")
      // 印がない、または念のため 3 時間を過ぎたら止める
      if (!flag.exists() || System.currentTimeMillis() - startedAt > 3 * 60 * 60 * 1000L) {
        stopSelf()
        return
      }
      handler.postDelayed(this, 30_000)
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val nm = getSystemService(NotificationManager::class.java)
    nm.createNotificationChannel(
      NotificationChannel(CHANNEL, "払戻の見張り", NotificationManager.IMPORTANCE_LOW)
    )
    val open = PendingIntent.getActivity(
      this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE
    )
    val n: Notification = Notification.Builder(this, CHANNEL)
      .setContentTitle("払戻を待っています")
      .setContentText("確定したら知らせます")
      .setSmallIcon(R.mipmap.ic_launcher)
      .setContentIntent(open)
      .setOngoing(true)
      .build()
    if (Build.VERSION.SDK_INT >= 29) {
      startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    } else {
      startForeground(ID, n)
    }
    handler.removeCallbacks(check)
    handler.postDelayed(check, 30_000)
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    handler.removeCallbacks(check)
    super.onDestroy()
  }

  companion object {
    const val CHANNEL = "payout_watch"
    const val ID = 4401
  }
}
