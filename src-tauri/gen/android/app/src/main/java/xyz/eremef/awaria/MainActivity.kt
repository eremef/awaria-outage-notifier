package xyz.eremef.awaria

import android.os.Bundle
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

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
