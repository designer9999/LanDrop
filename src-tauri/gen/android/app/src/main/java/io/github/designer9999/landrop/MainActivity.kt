package io.github.designer9999.landrop

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.View
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    WindowCompat.setDecorFitsSystemWindows(window, false)
    super.onCreate(savedInstanceState)
    applySystemBarInsets()
    requestNotificationPermission()
    ContextCompat.startForegroundService(this, Intent(this, LanDropForegroundService::class.java))
  }

  private fun applySystemBarInsets() {
    val content = findViewById<View>(android.R.id.content) ?: return
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      insets
    }
    ViewCompat.requestApplyInsets(content)
  }

  private fun requestNotificationPermission() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
    requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 29171)
  }
}
