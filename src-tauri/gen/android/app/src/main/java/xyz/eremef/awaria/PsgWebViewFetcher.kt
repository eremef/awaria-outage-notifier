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
            return webViewResult
        }

        // 4. LAST RESORT: Return stale cache if everything else failed
        if (cachedHtml != null) {
            Log.w(TAG, "All fetch methods failed. Returning STALE cache data (last good state).")
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

    private fun executePost(cookies: String, body: String): String? {
        return try {
            val conn = URL(PSG_URL).openConnection() as HttpURLConnection
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("User-Agent", MOBILE_USER_AGENT)
            conn.setRequestProperty("Cookie", cookies)
            conn.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
            conn.setRequestProperty("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            conn.setRequestProperty("Accept-Language", "pl-PL,pl;q=0.9,en-US;q=0.8,en;q=0.7")
            conn.connectTimeout = 10000
            conn.readTimeout = 10000
            conn.instanceFollowRedirects = true

            conn.outputStream.use { os ->
                os.write(body.toByteArray(kotlin.text.Charsets.UTF_8))
            }

            val code = conn.responseCode
            if (code in 200..299) {
                val html = conn.inputStream.bufferedReader().use { it.readText() }
                conn.disconnect()
                html
            } else {
                Log.d(TAG, "Direct POST failed: HTTP $code")
                conn.disconnect()
                null
            }
        } catch (e: Exception) {
            Log.d(TAG, "Direct POST error: ${e.message}")
            null
        }
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

        val activeHtml = executePost(cookies, "state=active&sort_col=shutdownDateTime&sort_ord=asc&title=") ?: return null
        val plannedHtml = executePost(cookies, "state=disabled&sort_col=shutdownDateTime&sort_ord=asc&title=") ?: return null

        val combinedHtml = "$activeHtml\n<hr>\n$plannedHtml"

        return if (combinedHtml.contains("supply-interruptions") || combinedHtml.contains("województwo") || combinedHtml.contains("Polska Spółka Gazownictwa") || combinedHtml.contains("Przerwy w dostawie gazu")) {
            combinedHtml
        } else {
            Log.w(TAG, "Direct fetch returned HTML but no outage table found")
            null
        }
    }

    /**
     * Loads the PSG page in a hidden WebView to solve Cloudflare challenge,
     * then extracts the HTML and caches cookies.
     */
    private suspend fun fetchViaWebView(context: Context): String? {
        val deferred = CompletableDeferred<String?>()
        var webView: WebView? = null

        try {
            withContext(Dispatchers.Main) {
                try {
                    val wv = WebView(context).apply {
                        // Give the WebView a physical size to satisfy scripts that check for visibility
                        layoutParams = ViewGroup.LayoutParams(1080, 1920)
                        
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        settings.useWideViewPort = true
                        settings.loadWithOverviewMode = true
                        settings.userAgentString = MOBILE_USER_AGENT
                    }
                    webView = wv

                    CookieManager.getInstance().setAcceptCookie(true)

                    wv.webViewClient = object : WebViewClient() {
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
                                         let activeCaptured = sessionStorage.getItem('psg_activeCaptured') === 'true';
                                         let plannedCaptured = sessionStorage.getItem('psg_plannedCaptured') === 'true';
                                         let activeHtml = sessionStorage.getItem('psg_activeHtml') || '';
                                         let plannedHtml = sessionStorage.getItem('psg_plannedHtml') || '';
                                         
                                         let startTimeStr = sessionStorage.getItem('psg_startTime');
                                         if (!startTimeStr) {
                                             startTimeStr = Date.now().toString();
                                             sessionStorage.setItem('psg_startTime', startTimeStr);
                                             sessionStorage.setItem('psg_lastActionTime', startTimeStr);
                                         }
                                         let startTime = parseInt(startTimeStr);

                                         function isTableMatchingState(state) {
                                             const container = document.getElementById('supply-interruptions-filter-form') || document.body;
                                             const text = container.innerText || '';
                                             
                                             if (state === 'active') {
                                                 if (text.includes('Brak trwających') || text.includes('Brak przerw')) {
                                                     return true;
                                                 }
                                             } else {
                                                 if (text.includes('Brak planowanych')) {
                                                     return true;
                                                 }
                                             }
                                             
                                             const rows = document.querySelectorAll('table tr');
                                             for (let i = 1; i < rows.length; i++) {
                                                 const rowText = rows[i].innerText || '';
                                                 if (state === 'active') {
                                                     if (rowText.toLowerCase().includes('awaria') || rowText.toLowerCase().includes('aktywna')) {
                                                         return true;
                                                     }
                                                 } else {
                                                     if (rowText.toLowerCase().includes('planowane') || rowText.toLowerCase().includes('planowana')) {
                                                         return true;
                                                     }
                                                 }
                                             }
                                             
                                             // Fallback: if we have waited more than 5 seconds after clicking, assume it loaded
                                             const lastAction = parseInt(sessionStorage.getItem('psg_lastActionTime') || '0');
                                             if (lastAction > 0 && (Date.now() - lastAction > 5000)) {
                                                 console.log('PSG-FETCH: Matching state fallback triggered');
                                                 return true;
                                             }
                                             
                                             return false;
                                         }

                                         const now = Date.now();
                                         const body = document.body ? document.body.innerHTML : '';

                                         if (body.includes('Checking your browser') || body.includes('Verify you are human') || body.includes('Cloudflare')) {
                                             return 'waiting';
                                         }

                                         const checkbox0 = document.getElementById('checkbox0'); // active (aktywna)
                                         const checkbox1 = document.getElementById('checkbox1'); // planned (planowana)

                                         const isActiveChecked = checkbox0 && checkbox0.checked;
                                         const isPlannedChecked = checkbox1 && checkbox1.checked;

                                         if (isActiveChecked) {
                                             if (!activeCaptured && isTableMatchingState('active')) {
                                                 activeHtml = body;
                                                 activeCaptured = true;
                                                 sessionStorage.setItem('psg_activeHtml', body);
                                                 sessionStorage.setItem('psg_activeCaptured', 'true');
                                             }
                                         } else if (isPlannedChecked) {
                                             if (!plannedCaptured && isTableMatchingState('planned')) {
                                                 plannedHtml = body;
                                                 plannedCaptured = true;
                                                 sessionStorage.setItem('psg_plannedHtml', body);
                                                 sessionStorage.setItem('psg_plannedCaptured', 'true');
                                             }
                                         }

                                         if (activeCaptured && plannedCaptured) {
                                             sessionStorage.removeItem('psg_activeCaptured');
                                             sessionStorage.removeItem('psg_plannedCaptured');
                                             sessionStorage.removeItem('psg_activeHtml');
                                             sessionStorage.removeItem('psg_plannedHtml');
                                             sessionStorage.removeItem('psg_startTime');
                                             sessionStorage.removeItem('psg_lastActionTime');
                                             return (activeHtml || '') + "\n<hr>\n" + (plannedHtml || '');
                                         }

                                         if (!activeCaptured) {
                                             if (!isActiveChecked && checkbox0) {
                                                 sessionStorage.setItem('psg_lastActionTime', now.toString());
                                                 checkbox0.click();
                                                 checkbox0.dispatchEvent(new Event('change', { bubbles: true }));
                                             }
                                             return 'waiting';
                                         }

                                         if (!plannedCaptured) {
                                             if (!isPlannedChecked && checkbox1) {
                                                 sessionStorage.setItem('psg_lastActionTime', now.toString());
                                                 checkbox1.click();
                                                 checkbox1.dispatchEvent(new Event('change', { bubbles: true }));
                                             }
                                             return 'waiting';
                                         }

                                         if (now - startTime > 45000) {
                                             sessionStorage.removeItem('psg_activeCaptured');
                                             sessionStorage.removeItem('psg_plannedCaptured');
                                             sessionStorage.removeItem('psg_activeHtml');
                                             sessionStorage.removeItem('psg_plannedHtml');
                                             sessionStorage.removeItem('psg_startTime');
                                             sessionStorage.removeItem('psg_lastActionTime');
                                             return (activeHtml || '') + "\n<hr>\n" + (plannedHtml || '');
                                         }

                                         return 'waiting';
                                     })()
                                    """.trimIndent()
                                 ) { result ->
                                     if (deferred.isCompleted) return@evaluateJavascript
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

                    wv.loadUrl(PSG_URL)
                } catch (e: Exception) {
                    Log.e(TAG, "WebView creation error: ${e.message}")
                    deferred.complete(null)
                }
            }

            return withTimeoutOrNull(TIMEOUT_MS) { deferred.await() }
        } finally {
            if (!deferred.isCompleted) {
                deferred.cancel()
            }
            withContext(Dispatchers.Main) {
                try {
                    webView?.stopLoading()
                    webView?.destroy()
                } catch (e: Exception) {
                    Log.e(TAG, "Error cleaning up WebView: ${e.message}")
                }
            }
        }
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
                val type = cells[6].lowercase()
                
                // Skip completed outages
                if (status.contains("zakończona") || status.contains("zakonczona")) {
                    continue
                }
                
                val isAwaria = type.contains("awaria")

                // Skip expired outages (unless it's an active failure without a clear end date)
                if (!isAwaria && isExpired(cells[4])) {
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

    private fun stripStreetPrefixes(s: String): String {
        val prefixes = listOf(
            "ulica", "ul",
            "plac", "pl",
            "aleja", "al",
            "osiedle", "os",
            "rondo", "skwer"
        )
        for (prefix in prefixes) {
            if (s.startsWith(prefix) && s.length > prefix.length) {
                return s.substring(prefix.length)
            }
        }
        return s
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

                    val cleanSettingsStreet = stripStreetPrefixes(normSettingsStreet)
                    val cleanOutageArea = stripStreetPrefixes(normOutageArea)

                    val streetMatch = normSettingsStreet.isEmpty() || isLocalityWide || 
                                     normOutageArea.contains(normSettingsStreet) ||
                                     (cleanSettingsStreet.isNotEmpty() && 
                                      (normOutageArea.contains(cleanSettingsStreet) || cleanOutageArea.contains(cleanSettingsStreet)))

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
        val cleanStr = endDate.replace("godz.", "").replace("\\s+".toRegex(), " ").trim()
        return try {
            val formats = listOf(
                SimpleDateFormat("dd.MM.yyyy HH:mm", Locale.getDefault()),
                SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault()),
                SimpleDateFormat("dd.MM.yyyy", Locale.getDefault()),
                SimpleDateFormat("yyyy-MM-dd", Locale.getDefault())
            )
            for (fmt in formats) {
                try {
                    val date = fmt.parse(cleanStr)
                    if (date != null) return date.before(java.util.Date())
                } catch (_: Exception) { }
            }
            false
        } catch (_: Exception) {
            false
        }
    }
}
