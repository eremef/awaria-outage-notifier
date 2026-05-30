package xyz.eremef.awaria

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import androidx.annotation.Keep
import android.webkit.WebViewClient
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.net.HttpURLConnection
import java.net.URL

@Keep
object GpecWebViewFetcher {
    private const val TAG = "GpecWebViewFetcher"
    private const val GPEC_URL = "https://grupagpec.pl/przerwy-w-dostawie/"
    private const val TIMEOUT_MS = 60000L // 60 seconds

    private const val PREFS_NAME = "xyz.eremef.awaria.GpecCache"
    private const val KEY_HTML = "cached_html"
    private const val KEY_HTML_TIME = "html_cache_time"
    private const val HTML_TTL_MS = 60 * 60 * 1000L // 1 hour

    private const val MOBILE_USER_AGENT = "Mozilla/5.0 (Linux; Android 13; SM-G998B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"

    @Keep
    @JvmStatic
    fun fetchHtmlNative(context: Context): String? {
        return kotlinx.coroutines.runBlocking {
            fetchHtml(context)
        }
    }

    private suspend fun fetchHtml(context: Context): String? {
        val now = System.currentTimeMillis()
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        
        // 1. Try persistent HTML cache (1 hour)
        val cachedHtml = prefs.getString(KEY_HTML, null)
        val htmlTime = prefs.getLong(KEY_HTML_TIME, 0L)
        if (cachedHtml != null && now - htmlTime < HTML_TTL_MS) {
            Log.i(TAG, "Using persistent HTML cache (1h TTL)")
            return cachedHtml
        }

        // 2. WebView fetch (Cloudflare challenge)
        Log.i(TAG, "Fetching via WebView")
        val webViewResult = fetchViaWebView(context)
        if (webViewResult != null) {
            saveHtmlCache(context, webViewResult)
            return webViewResult
        }

        // 3. Fallback to stale cache
        if (cachedHtml != null) {
            Log.w(TAG, "WebView failed. Returning STALE cache data.")
            return cachedHtml
        }

        return null
    }

    private fun saveHtmlCache(context: Context, html: String) {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit()
            .putString(KEY_HTML, html)
            .putLong(KEY_HTML_TIME, System.currentTimeMillis())
            .apply()
    }

    private suspend fun fetchViaWebView(context: Context): String? {
        val deferred = CompletableDeferred<String?>()

        withContext(Dispatchers.Main) {
            try {
                val webView = WebView(context).apply {
                    layoutParams = ViewGroup.LayoutParams(1080, 1920)
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = true
                    settings.useWideViewPort = true
                    settings.loadWithOverviewMode = true
                    settings.userAgentString = MOBILE_USER_AGENT
                }

                CookieManager.getInstance().setAcceptCookie(true)

                webView.webViewClient = object : WebViewClient() {
                    override fun onPageFinished(view: WebView, url: String) {
                        Log.d(TAG, "GPEC-FETCH: Page finished loading: ${'$'}url")
                        
                        var pollAttempts = 0
                        val maxPollAttempts = 90

                        fun pollState() {
                            if (deferred.isCompleted) return
                            pollAttempts++
                            if (pollAttempts > maxPollAttempts) {
                                Log.e(TAG, "GPEC-FETCH: Timeout waiting for DOM elements")
                                deferred.complete(null)
                                return
                            }

                             view.evaluateJavascript(
                                """
                                (function() {
                                    const allowBtn = document.getElementById('CybotCookiebotDialogBodyLevelButtonLevelOptinAllowAll');
                                    if (allowBtn && allowBtn.offsetParent !== null) {
                                        allowBtn.click();
                                    }

                                    const bodyHtml = document.body ? document.body.innerHTML : '';
                                    const bodyText = document.body ? document.body.innerText : '';
                                    
                                    if (document.title.includes('Just a moment') || bodyHtml.includes('Checking your browser') || bodyHtml.includes('Verify you are human')) {
                                        return 'waiting';
                                    }
                                    
                                    if (document.querySelector('.no-acc-info') || document.querySelector('.cloud-info') || document.querySelector('.dashed') || document.querySelector('.grupagpec-pl-przerwy-w-dostawie') || bodyText.includes('Brak przerw w dostawie') || bodyText.includes('Brak przerw')) {
                                        let relevantHtml = '';
                                        const noAcc = document.querySelector('.no-acc-info');
                                        if (noAcc) relevantHtml += noAcc.outerHTML + '\n';
                                        
                                        const cloudInfos = document.querySelectorAll('.cloud-info');
                                        cloudInfos.forEach(el => relevantHtml += el.outerHTML + '\n');

                                        const dashed = document.querySelectorAll('.dashed');
                                        dashed.forEach(el => relevantHtml += el.outerHTML + '\n');
                                        
                                        if (!relevantHtml.trim()) {
                                            relevantHtml = 'Brak przerw';
                                        }
                                        return relevantHtml;
                                    }

                                    return 'waiting';
                                })()
                                """.trimIndent()
                            ) { result ->
                                val res = if (result != null && result != "null") unescapeJsString(result) else "waiting"
                                if (res == "waiting") {
                                    Handler(Looper.getMainLooper()).postDelayed({ pollState() }, 1000)
                                } else {
                                    if (res.isNotEmpty()) {
                                        Log.d(TAG, "GPEC-FETCH: Done! Length: ${'$'}{res.length}")
                                        deferred.complete(res)
                                    } else {
                                        deferred.complete(null)
                                    }
                                }
                            }
                        }
                        
                        pollState()
                    }
                }

                webView.loadUrl(GPEC_URL)
            } catch (e: Exception) {
                Log.e(TAG, "WebView creation error: ${'$'}{e.message}")
                deferred.complete(null)
            }
        }

        return withTimeoutOrNull(TIMEOUT_MS) { deferred.await() }
    }

    private fun unescapeJsString(jsString: String): String {
        var s = jsString
        if (s.startsWith("\"") && s.endsWith("\"")) {
            s = s.substring(1, s.length - 1)
        }
        return s.replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\u003C", "<")
            .replace("\\u003c", "<")
            .replace("\\u003E", ">")
            .replace("\\u003e", ">")
            .replace("\\u0026", "&")
            .replace("\\/", "/")
    }
}
