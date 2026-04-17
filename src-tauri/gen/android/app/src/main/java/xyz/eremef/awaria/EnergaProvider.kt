package xyz.eremef.awaria

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class EnergaProvider : IOutageProvider {
    companion object {
        private const val TAG = "AwariaEnerga"
        private const val URL_CACHE_TTL = 3600000L // 1 hour
    }

    override val id: String = "energa"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isInEnergaRegion(it) }
        if (relevantSettings.isEmpty()) return 0

        var totalCount = 0
        try {
            val apiUrl = getOrFetchApiUrl(context) ?: return 0
            val response = WidgetUtils.fetchJson(URL(apiUrl))
            val json = JSONObject(response)
            val shutdowns =
                json.optJSONObject("document")
                    ?.optJSONObject("payload")
                    ?.optJSONArray("shutdowns")
                    ?: return 0

            // Pre-compile matchers for all relevant settings
            val matchers = relevantSettings.map { it to WidgetUtils.CompiledMatcher(it) }
            val counts = IntArray(relevantSettings.size) { 0 }

            // Energa provides a global list, so we fetch once and filter locally for all addresses
            for (i in 0 until shutdowns.length()) {
                val s = shutdowns.getJSONObject(i)
                val message = s.optString("message", "")
                val areasArray = s.optJSONArray("areas")
                val areas = if (areasArray != null) {
                    List(areasArray.length()) { areasArray.getString(it) }
                } else null

                for (idx in matchers.indices) {
                    if (matchers[idx].second.matchesFull(message, areas)) {
                        counts[idx]++
                    }
                }
            }
            totalCount = counts.sum()
        } catch (e: Exception) {
            Log.e(TAG, "Energa sync failed", e)
        }
        return totalCount
    }

    private fun getOrFetchApiUrl(context: Context): String? {
        val prefs = context.getSharedPreferences("awaria_cache", Context.MODE_PRIVATE)
        val cachedUrl = prefs.getString("energa_api_url", null)
        val lastFetch = prefs.getLong("energa_url_fetch_time", 0L)
        val now = System.currentTimeMillis()

        if (cachedUrl != null && (now - lastFetch < URL_CACHE_TTL)) {
            return cachedUrl
        }

        return try {
            val url = URL("https://www.energa-operator.pl/uslugi/awarie-i-wylaczenia/wylaczenia-planowane")
            val html = url.readText()
            val regex = Regex("""data-shutdowns="([^"]+)"""")
            val match = regex.find(html)
            match?.groupValues?.get(1)?.let { 
                val fullUrl = "https://energa-operator.pl$it"
                prefs.edit().apply {
                    putString("energa_api_url", fullUrl)
                    putLong("energa_url_fetch_time", now)
                    apply()
                }
                fullUrl
            }
        } catch (e: Exception) {
            null
        }
    }

    private fun isInEnergaRegion(settings: WidgetSettings): Boolean {
        val v = settings.voivodeship.lowercase()
        return v.contains("pomorskie") ||
                v.contains("warmińsko") ||
                v.contains("zachodniopomorskie") ||
                v.contains("wielkopolskie") ||
                v.contains("kujawsko") ||
                v.contains("mazowieckie")
    }
}
