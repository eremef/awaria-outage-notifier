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
@Keep
object PsgWebViewFetcher {
    private const val TAG = "PsgWebViewFetcher"
    private const val PSG_URL = "https://www.psgaz.pl/przerwy-w-dostawie-gazu"
    private const val TIMEOUT_MS = 60000L // 60 seconds

    private const val PREFS_NAME = "xyz.eremef.awaria.PsgCache"
    private const val KEY_HTML = "cached_html"
    private const val KEY_HTML_TIME = "html_cache_time"
    private const val KEY_COOKIES = "cached_cookies"
    private const val KEY_COOKIES_TIME = "cookies_cache_time"
    private const val HTML_TTL_MS = 60 * 60 * 1000L // 1 hour
    private const val COOKIE_TTL_MS = 25 * 60 * 1000L // 25 minutes

    private const val MOBILE_USER_AGENT = "Mozilla/5.0 (Linux; Android 13; SM-G998B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"

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
    
    @Keep
    @JvmStatic
    fun fetchHtmlNative(context: Context): String? {
        return kotlinx.coroutines.runBlocking {
            fetchHtml(context)
        }
    }

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
        val now = System.currentTimeMillis()
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        
        // 1. Try persistent HTML cache (1 hour)
        val cachedHtml = prefs.getString(KEY_HTML, null)
        val htmlTime = prefs.getLong(KEY_HTML_TIME, 0L)
        if (cachedHtml != null && now - htmlTime < HTML_TTL_MS) {
            Log.i(TAG, "Using persistent HTML cache (1h TTL)")
            return cachedHtml
        }

        // 2. Try direct fetch with cached cookies
        val directResult = tryDirectFetch(context)
        if (directResult != null) {
            Log.i(TAG, "Direct fetch succeeded (using cached cookies)")
            saveHtmlCache(context, directResult)
            return directResult
        }

        // 3. WebView fallback
        Log.i(TAG, "Direct fetch failed, falling back to WebView")
        val webViewResult = fetchViaWebView(context)
        if (webViewResult != null) {
            saveHtmlCache(context, webViewResult)
        }
        return webViewResult
    }

    private fun saveHtmlCache(context: Context, html: String) {
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit()
            .putString(KEY_HTML, html)
            .putLong(KEY_HTML_TIME, System.currentTimeMillis())
            .apply()
    }

    /**
     * Try fetching the PSG page directly using HttpURLConnection with cached cookies.
     */
    private fun tryDirectFetch(context: Context): String? {
        val now = System.currentTimeMillis()
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val cookies = prefs.getString(KEY_COOKIES, null) ?: return null
        val cookieTime = prefs.getLong(KEY_COOKIES_TIME, 0L)
        
        if (now - cookieTime > COOKIE_TTL_MS) {
            Log.d(TAG, "Cookie cache expired")
            return null
        }

        return try {
            val conn = URL(PSG_URL).openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("User-Agent", MOBILE_USER_AGENT)
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
                    // Give the WebView a physical size to satisfy scripts that check for visibility
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
                        Log.d(TAG, "PSG-FETCH: Page finished loading: $url")
                        
                        var pollAttempts = 0
                        val maxPollAttempts = 90 // 90 seconds max

                        fun pollState() {
                            if (deferred.isCompleted) return
                            pollAttempts++
                            if (pollAttempts > maxPollAttempts) {
                                Log.e(TAG, "PSG-FETCH: Timeout waiting for state machine")
                                deferred.complete(null)
                                return
                            }

                            view.evaluateJavascript(
                                """
                                (function() {
                                    function trySwitch() {
                                        const checkbox1 = document.getElementById('checkbox1');
                                        if (checkbox1 && !checkbox1.checked) {
                                            checkbox1.click();
                                            window._waiting = true;
                                            window._lastSwitchTime = Date.now();
                                            setTimeout(() => { window._waiting = false; }, 3000);
                                            return true;
                                        }
                                        
                                        const interactive = Array.from(document.querySelectorAll('button, a, span, li, label, input'));
                                        const plannedBtn = interactive.find(el => {
                                            const text = el.innerText || (el.value && typeof el.value === 'string' ? el.value : '');
                                            return /planowane/i.test(text.trim());
                                        });
                                        
                                        if (plannedBtn) {
                                            const isAlreadyActive = plannedBtn.classList.contains('active') || 
                                                                  plannedBtn.classList.contains('selected') ||
                                                                  (plannedBtn.parentElement && plannedBtn.parentElement.classList.contains('active')) ||
                                                                  (plannedBtn.tagName === 'INPUT' && plannedBtn.checked);
                                            
                                            if (!isAlreadyActive) {
                                                plannedBtn.click();
                                                window._waiting = true;
                                                window._lastSwitchTime = Date.now();
                                                setTimeout(() => { window._waiting = false; }, 3000);
                                                return true;
                                            }
                                        }
                                        return false;
                                    }
                                    
                                    if (window._psgState === undefined) {
                                        window._psgState = 'capture_active';
                                        window._activeHtml = '';
                                        window._plannedHtml = '';
                                        window._startTime = Date.now();
                                    }

                                    const now = Date.now();
                                    const text = document.body ? document.body.innerText : '';
                                    const body = document.body ? document.body.innerHTML : '';
                                    const hasTable = body.includes('<table') || body.includes('<tbody>') || body.includes('supply-interruptions');
                                    const isBrak = text.includes('Brak') || text.includes('przerw');

                                    if (window._waiting && (now - window._lastSwitchTime < 5000)) {
                                        return 'waiting'; 
                                    }
                                    window._waiting = false;

                                    switch(window._psgState) {
                                        case 'capture_active':
                                            if (hasTable || isBrak || (now - window._startTime > 5000)) {
                                                window._activeHtml = body;
                                                window._psgState = 'switching';
                                            }
                                            return 'waiting';

                                        case 'switching':
                                            if (trySwitch()) {
                                                window._psgState = 'capture_planned';
                                                window._waiting = true;
                                                window._lastSwitchTime = now;
                                            } else {
                                                window._psgState = 'done';
                                            }
                                            return 'waiting';

                                        case 'capture_planned':
                                            if (hasTable || isBrak || (now - window._lastSwitchTime > 5000)) {
                                                window._plannedHtml = body;
                                                window._psgState = 'done';
                                            }
                                            return 'waiting';

                                        case 'done':
                                            return (window._activeHtml || '') + "\n<hr>\n" + (window._plannedHtml || '');
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
                                        Log.d(TAG, "PSG-FETCH: Done! Length: ${res.length}")
                                        cacheCookies(context)
                                        deferred.complete(res)
                                    } else {
                                        deferred.complete(null)
                                    }
                                }
                            }
                        }
                        
                        pollState()
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

    private fun cacheCookies(context: Context) {
        try {
            val cookies = CookieManager.getInstance().getCookie(PSG_URL)
            if (cookies != null) {
                context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit()
                    .putString(KEY_COOKIES, cookies)
                    .putLong(KEY_COOKIES_TIME, System.currentTimeMillis())
                    .apply()
                Log.i(TAG, "Cached cookies persistently")
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
        val trPattern = Pattern.compile("<tr[^>]*?>(.*?)</tr>", Pattern.DOTALL)
        val tdPattern = Pattern.compile("<td[^>]*?>(.*?)</td>", Pattern.DOTALL)
        val tagStripper = Pattern.compile("<[^>]+>")

        val trMatcher = trPattern.matcher(html)
        var rowCount = 0
        while (trMatcher.find()) {
            rowCount++
            val rowHtml = trMatcher.group(1) ?: continue
            val cells = mutableListOf<String>()
            val tdMatcher = tdPattern.matcher(rowHtml)
            while (tdMatcher.find()) {
                val cellHtml = tdMatcher.group(1) ?: ""
                val text = tagStripper.matcher(cellHtml).replaceAll("")
                    .replace("&nbsp;", " ")
                    .replace("&amp;", "&")
                    .replace("\\s+".toRegex(), " ")
                    .trim()
                cells.add(text)
            }

            // Relaxed check: at least 7 columns (Status might be missing or optional in some views)
            if (cells.size >= 7) {
                val status = if (cells.size >= 8) cells[7].lowercase() else ""
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
                        status = status
                    )
                )
            }
        }

        Log.i(TAG, "PSG-PARSE: Found $rowCount HTML rows, parsed ${outages.size} PSG outages")
        return outages
    }

    /**
     * Normalizes a string by converting to lowercase, replacing Polish characters,
     * and keeping only alphanumeric characters.
     */
    private fun normalize(s: String): String {
        return s.lowercase().map { c ->
            when (c) {
                'ą' -> 'a'
                'ć' -> 'c'
                'ę' -> 'e'
                'ł' -> 'l'
                'ń' -> 'n'
                'ó' -> 'o'
                'ś' -> 's'
                'ź', 'ż' -> 'z'
                else -> c
            }
        }.filter { it.isLetterOrDigit() }.joinToString("")
    }

    fun countMatchingOutages(outages: List<PsgOutage>, settingsList: List<WidgetSettings>): Int {
        var count = 0

        for (outage in outages) {
            val normOutageCity = normalize(outage.city)
            val normOutageArea = normalize(outage.area)

            for (settings in settingsList) {
                if (!settings.isActive) continue

                val normSettingsCity = normalize(settings.cityName)
                val cityMatch = normOutageCity == normSettingsCity || 
                               normOutageCity.contains(normSettingsCity) || 
                               normSettingsCity.contains(normOutageCity)

                // Sometimes city is in the area field and city field contains gmina or district
                val cityInArea = !cityMatch && (normOutageArea.contains(normSettingsCity) || normSettingsCity.contains(normOutageArea))

                if (cityMatch || cityInArea) {
                    val normSettingsStreet = if (settings.streetName1.isNotEmpty()) normalize(settings.streetName1) else ""

                    // Check if the outage is locality-wide (e.g. "m. Brzezówka", "cała miejscowość")
                    val isLocalityWide = run {
                        val cityWithPrefix = "m$normSettingsCity"
                        normOutageArea == normSettingsCity
                                || normOutageArea == cityWithPrefix
                                || normOutageArea.contains(cityWithPrefix)
                                || normOutageArea.contains("calamiejscowosc")
                                || normOutageArea.contains("calyobszarmiejscowosci")
                    }

                    val streetMatch = normSettingsStreet.isEmpty() || isLocalityWide || normOutageArea.contains(normSettingsStreet)

                    if (streetMatch) {
                        Log.d(TAG, "[PSG] Match found for ${settings.cityName}: city=${outage.city}, area=${outage.area}")
                        count++
                        break // Count each outage only once for these settings
                    } else {
                        Log.d(TAG, "[PSG] City match but street mismatch for ${settings.cityName}: expected=${settings.streetName1}, area=${outage.area}")
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
