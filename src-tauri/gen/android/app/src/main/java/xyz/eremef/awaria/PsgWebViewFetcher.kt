package xyz.eremef.awaria

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.regex.Pattern

/**
 * Fetches PSG gas outage data by loading the page in a WebView to bypass Cloudflare,
 * then parses the HTML table directly in Kotlin.
 *
 * Flow:
 * 1. Try a direct HTTP fetch first (using cached cf_clearance cookie)
 * 2. If that fails (403), use WebView to load the page and solve the CF challenge
 * 3. Extract HTML from the WebView and cache the cf_clearance cookie
 * 4. Parse the outage table from the HTML
 */
object PsgWebViewFetcher {
    private const val TAG = "PsgWebViewFetcher"
    private const val PSG_URL = "https://www.psgaz.pl/przerwy-w-dostawie-gazu"
    private const val TIMEOUT_MS = 60000L // 60 seconds

    // Cookie cache
    private var cachedCfClearance: String? = null
    private var cachedCookies: String? = null
    private var cookieCacheTime = 0L
    private const val COOKIE_TTL_MS = 25 * 60 * 1000L // 25 minutes

    data class PsgOutage(
        val province: String,
        val city: String,
        val area: String,
        val startDate: String,
        val endDate: String,
        val info: String,
        val type: String,
        val status: String
    )

    /**
     * Main entry point: fetches and counts PSG outages matching the given settings.
     */
    suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val activeSettings = settingsList.filter { it.isActive }
        if (activeSettings.isEmpty()) return 0

        val html = fetchHtml(context)
        if (html.isNullOrEmpty()) {
            Log.w(TAG, "Failed to fetch PSG HTML")
            return -1
        }

        val outages = parseOutages(html)
        return countMatchingOutages(outages, activeSettings)
    }

    /**
     * Fetches the PSG page HTML, trying direct fetch first, then WebView fallback.
     */
    private suspend fun fetchHtml(context: Context): String? {
        // 1. Try direct fetch with cached cookies
        val directResult = tryDirectFetch()
        if (directResult != null) {
            Log.i(TAG, "Direct fetch succeeded (using cached cookies)")
            return directResult
        }

        // 2. WebView fallback
        Log.i(TAG, "Direct fetch failed, falling back to WebView")
        return fetchViaWebView(context)
    }

    /**
     * Try fetching the PSG page directly using HttpURLConnection with cached cookies.
     */
    private fun tryDirectFetch(): String? {
        val now = System.currentTimeMillis()
        val cookies = cachedCookies ?: return null
        if (now - cookieCacheTime > COOKIE_TTL_MS) {
            Log.d(TAG, "Cookie cache expired")
            cachedCookies = null
            cachedCfClearance = null
            return null
        }

        return try {
            val conn = URL(PSG_URL).openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("User-Agent", "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36")
            conn.setRequestProperty("Cookie", cookies)
            conn.setRequestProperty("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            conn.setRequestProperty("Accept-Language", "pl-PL,pl;q=0.9,en-US;q=0.8,en;q=0.7")
            conn.connectTimeout = 10000
            conn.readTimeout = 10000
            conn.instanceFollowRedirects = true

            val code = conn.responseCode
            if (code in 200..299) {
                val html = conn.inputStream.bufferedReader().use { it.readText() }
                conn.disconnect()
                // Verify the HTML actually contains the outage table
                if (html.contains("supply-interruptions") || html.contains("województwo") || html.contains("Polska Spółka Gazownictwa") || html.contains("Przerwy w dostawie gazu")) {
                    html
                } else {
                    Log.w(TAG, "Direct fetch returned $code but no outage table found")
                    null
                }
            } else {
                Log.d(TAG, "Direct fetch returned HTTP $code")
                conn.disconnect()
                null
            }
        } catch (e: Exception) {
            Log.d(TAG, "Direct fetch error: ${e.message}")
            null
        }
    }

    /**
     * Loads the PSG page in a hidden WebView to solve Cloudflare challenge,
     * then extracts the HTML and caches cookies.
     */
    private suspend fun fetchViaWebView(context: Context): String? {
        val deferred = CompletableDeferred<String?>()

        withContext(Dispatchers.Main) {
            try {
                val webView = WebView(context).apply {
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = true
                    settings.userAgentString = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36"
                }

                CookieManager.getInstance().setAcceptCookie(true)

                webView.webViewClient = object : WebViewClient() {
                    override fun onPageFinished(view: WebView, url: String) {
                        // Wait a moment for any JS to settle, then extract HTML
                        Handler(Looper.getMainLooper()).postDelayed({
                            view.evaluateJavascript(
                                "(function() { return document.documentElement.outerHTML; })()"
                            ) { html ->
                                try {
                                    if (html != null && html != "null") {
                                        // JS returns JSON-escaped string, unescape it
                                        val unescaped = unescapeJsString(html)
                                        if (unescaped.contains("województwo") || unescaped.contains("supply-interruptions") || unescaped.contains("Polska Spółka Gazownictwa") || unescaped.contains("Wyszukiwarka") || unescaped.contains("Przerwy w dostawie gazu") || unescaped.contains("Brak przerw")) {
                                            // Cache cookies for future direct fetches
                                            cacheCookies()
                                            deferred.complete(unescaped)
                                        } else {
                                            Log.w(TAG, "WebView loaded but no outage table found, might be CF challenge page")
                                            // Try again after more delay (CF challenge solving)
                                            Handler(Looper.getMainLooper()).postDelayed({
                                                view.evaluateJavascript(
                                                    "(function() { return document.documentElement.outerHTML; })()"
                                                ) { retryHtml ->
                                                    val retryUnescaped = if (retryHtml != null && retryHtml != "null") unescapeJsString(retryHtml) else null
                                                    if (retryUnescaped != null && (retryUnescaped.contains("województwo") || retryUnescaped.contains("supply-interruptions") || retryUnescaped.contains("Polska Spółka Gazownictwa") || retryUnescaped.contains("Przerwy w dostawie gazu"))) {
                                                        cacheCookies()
                                                        deferred.complete(retryUnescaped)
                                                    } else {
                                                        deferred.complete(null)
                                                    }
                                                    view.destroy()
                                                }
                                            }, 3000)
                                            return@evaluateJavascript
                                        }
                                    } else {
                                        deferred.complete(null)
                                    }
                                    view.destroy()
                                } catch (e: Exception) {
                                    Log.e(TAG, "Error extracting HTML: ${e.message}")
                                    deferred.complete(null)
                                    view.destroy()
                                }
                            }
                        }, 2000) // 2s delay for CF challenge to resolve
                    }

                    override fun onReceivedHttpError(
                        view: WebView,
                        request: WebResourceRequest,
                        errorResponse: WebResourceResponse
                    ) {
                        if (request.isForMainFrame) {
                            Log.e(TAG, "WebView HTTP error: ${errorResponse.statusCode}")
                        }
                    }
                }

                webView.loadUrl(PSG_URL)
            } catch (e: Exception) {
                Log.e(TAG, "WebView creation error: ${e.message}")
                deferred.complete(null)
            }
        }

        return withTimeoutOrNull(TIMEOUT_MS) { deferred.await() }
    }

    private fun cacheCookies() {
        try {
            val cookies = CookieManager.getInstance().getCookie(PSG_URL)
            if (cookies != null) {
                cachedCookies = cookies
                cookieCacheTime = System.currentTimeMillis()
                // Extract cf_clearance specifically
                val cfMatch = Regex("cf_clearance=([^;]+)").find(cookies)
                cachedCfClearance = cfMatch?.groupValues?.get(1)
                Log.i(TAG, "Cached cookies (cf_clearance=${cachedCfClearance != null})")
            }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to cache cookies: ${e.message}")
        }
    }

    private fun unescapeJsString(jsString: String): String {
        var s = jsString
        // Remove surrounding quotes
        if (s.startsWith("\"") && s.endsWith("\"")) {
            s = s.substring(1, s.length - 1)
        }
        // Unescape common JS escapes
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

    /**
     * Parses the PSG outage table from HTML.
     * Table columns: województwo, miejscowość, obszar, wyłączenie od, szacowany termin, info, typ, status
     */
    fun parseOutages(html: String): List<PsgOutage> {
        val outages = mutableListOf<PsgOutage>()

        // Find all <tr> rows and extract <td> cells
        val trPattern = Pattern.compile("<tr[^>]*>(.*?)</tr>", Pattern.DOTALL)
        val tdPattern = Pattern.compile("<td[^>]*>(.*?)</td>", Pattern.DOTALL)
        val tagStripper = Pattern.compile("<[^>]+>")

        val trMatcher = trPattern.matcher(html)
        while (trMatcher.find()) {
            val rowHtml = trMatcher.group(1) ?: continue
            val cells = mutableListOf<String>()
            val tdMatcher = tdPattern.matcher(rowHtml)
            while (tdMatcher.find()) {
                val cellHtml = tdMatcher.group(1) ?: ""
                val text = tagStripper.matcher(cellHtml).replaceAll("").trim()
                    .replace("\\s+".toRegex(), " ")
                cells.add(text)
            }

            if (cells.size >= 8) {
                val status = cells[7].lowercase()
                // Skip completed outages
                if (status.contains("zakończona") || status.contains("zakonczona")) {
                    continue
                }

                outages.add(
                    PsgOutage(
                        province = cells[0],
                        city = cells[1],
                        area = cells[2],
                        startDate = cells[3],
                        endDate = cells[4],
                        info = cells[5],
                        type = cells[6],
                        status = cells[7]
                    )
                )
            }
        }

        Log.i(TAG, "Parsed ${outages.size} active PSG outages")
        return outages
    }

    /**
     * Counts outages matching the user's configured addresses.
     */
    fun countMatchingOutages(outages: List<PsgOutage>, settingsList: List<WidgetSettings>): Int {
        var count = 0

        for (outage in outages) {
            for (settings in settingsList) {
                if (!settings.isActive) continue

                // City match
                if (outage.city.equals(settings.cityName, ignoreCase = true)) {
                    // Street match within the area field
                    if (settings.streetName1.isNotEmpty()) {
                        val matcher = WidgetUtils.CompiledMatcher(settings)
                        if (matcher.matchesStreet(outage.area)) {
                            count++
                            break // Count each outage only once
                        }
                    }
                }
            }
        }

        return count
    }

    /**
     * Checks if an outage's end date is in the past (expired).
     */
    private fun isExpired(endDate: String): Boolean {
        if (endDate.isEmpty() || endDate.contains("termin zostanie")) return false
        return try {
            val formats = listOf(
                SimpleDateFormat("dd.MM.yyyy HH:mm", Locale.getDefault()),
                SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault()),
                SimpleDateFormat("dd.MM.yyyy", Locale.getDefault())
            )
            for (fmt in formats) {
                try {
                    val date = fmt.parse(endDate)
                    if (date != null) return date.before(java.util.Date())
                } catch (_: Exception) { }
            }
            false
        } catch (_: Exception) {
            false
        }
    }
}
