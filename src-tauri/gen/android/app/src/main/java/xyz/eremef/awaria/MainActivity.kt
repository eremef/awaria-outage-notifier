package xyz.eremef.awaria

import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.ViewGroup
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Enable edge-to-edge for Android 15+ (API 35+) compatibility.
    // This replaces manual status/navigation bar color settings which are deprecated.
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT)
    )
    
    // Set layout in display cutout mode to use the full screen including notch areas.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      window.attributes.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
    }

    super.onCreate(savedInstanceState)
    try {
        android.util.Log.i("AWARIA", "Calling initVerifier from onCreate")
        WidgetUtils.initVerifier(this.applicationContext)
        android.util.Log.i("AWARIA", "initVerifier call completed")
    } catch (e: Exception) {
        android.util.Log.e("AWARIA", "Failed to call initVerifier: ${e.message}", e)
    }

    val decorView = this.window.decorView
    ViewCompat.setOnApplyWindowInsetsListener(decorView) { _, insets: WindowInsetsCompat ->
      val safeInsets = insets.getInsets(WindowInsetsCompat.Type.systemBars())
      val density = this.resources.displayMetrics.density
      val top = safeInsets.top / density
      val bottom = safeInsets.bottom / density

      findWebView(decorView as ViewGroup)?.let { webView ->
        if (!this.isFinishing && !this.isDestroyed) {
          webView.post {
            if (!this.isFinishing && !this.isDestroyed) {
              webView.evaluateJavascript(
                "document.documentElement.style.setProperty('--native-safe-area-inset-top', '${top}px');" +
                "document.documentElement.style.setProperty('--native-safe-area-inset-bottom', '${bottom}px');",
                null
              )
            }
          }
        }
      }
      insets
    }
  }

  private fun findWebView(view: ViewGroup): WebView? {
    for (i in 0 until view.childCount) {
      val child = view.getChildAt(i)
      if (child is WebView) return child
      if (child is ViewGroup) {
        val result = findWebView(child)
        if (result != null) return result
      }
    }
    return null
  }
}
