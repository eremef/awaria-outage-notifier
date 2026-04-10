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
    }

    override val id: String = "energa"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isInEnergaRegion(it) }
        if (relevantSettings.isEmpty()) return 0

        var totalCount = 0
        try {
            val apiUrl = fetchEnergaApiUrl() ?: return 0
            val response = WidgetUtils.fetchJson(URL(apiUrl))
            val json = JSONObject(response)
            val shutdowns =
                json.optJSONObject("document")
                    ?.optJSONObject("payload")
                    ?.optJSONArray("shutdowns")
                    ?: return 0

            for (settings in settingsList) {
                var count = 0
                for (i in 0 until shutdowns.length()) {
                    val s = shutdowns.getJSONObject(i)
                    val endStr = s.optString("endDate", "")
                    if (!DateUtils.isOutageActive(endStr)) continue
                    val message = s.optString("message", "")
                    val areas = s.optJSONArray("areas")

                    val cityMatch = WidgetUtils.wordMatch(message, settings.cityName)
                    var communeMatch = false
                    if (areas != null) {
                        for (j in 0 until areas.length()) {
                            if (WidgetUtils.wordMatch(areas.getString(j), settings.commune)) {
                                communeMatch = true
                                break
                            }
                        }
                    }

                    if (cityMatch &&
                        communeMatch &&
                        WidgetUtils.matchesStreetOnly(
                            message,
                            settings.streetName1,
                            settings.streetName2
                        )
                    )
                        count++
                }
                totalCount += count
            }
        } catch (e: Exception) {
            Log.e(TAG, "Energa sync failed", e)
        }
        return totalCount
    }

    private fun fetchEnergaApiUrl(): String? {
        return try {
            val url =
                URL(
                    "https://www.energa-operator.pl/uslugi/awarie-i-wylaczenia/wylaczenia-planowane"
                )
            val html = URL(url.toString()).readText()
            val regex = Regex("""data-shutdowns="([^"]+)"""")
            val match = regex.find(html)
            match?.groupValues?.get(1)?.let { "https://energa-operator.pl$it" }
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
