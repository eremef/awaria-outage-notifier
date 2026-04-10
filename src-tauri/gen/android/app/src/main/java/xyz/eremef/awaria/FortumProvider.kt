package xyz.eremef.awaria

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class FortumProvider : IOutageProvider {
    companion object {
        private const val TAG = "AwariaFortum"
    }

    override val id: String = "fortum"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val citiesUrl = URL("https://formularz.fortum.pl/api/v1/teryt/cities")
        val citiesJson =
            try {
                WidgetUtils.fetchJson(citiesUrl)
            } catch (e: Exception) {
                Log.e(TAG, "Failed to fetch Fortum cities", e)
                return 0
            }

        val citiesArray = org.json.JSONArray(citiesJson)
        val cityDataMap = mutableMapOf<String, Pair<String, String>>()
        for (i in 0 until citiesArray.length()) {
            val city = citiesArray.getJSONObject(i)
            val cityName = city.optString("cityName", "").lowercase()
            if (cityName.isNotEmpty()) {
                cityDataMap[cityName] =
                    Pair(city.optString("cityGuid", ""), city.opt("regionId")?.toString() ?: "")
            }
        }

        val cityGroups = settingsList.groupBy { it.cityName.lowercase() }
        var totalCount = 0

        for ((cityNameLower, addresses) in cityGroups) {
            val data = cityDataMap[cityNameLower] ?: continue
            val guid = data.first
            val rid = data.second
            if (guid.isEmpty() || rid.isEmpty()) continue

            try {
                val plannedUrl =
                    URL(
                        "https://formularz.fortum.pl/api/v1/switchoffs?cityGuid=$guid&regionId=$rid&current=false"
                    )
                val currentUrl =
                    URL(
                        "https://formularz.fortum.pl/api/v1/switchoffs?cityGuid=$guid&regionId=$rid&current=true"
                    )

                val plannedRes = WidgetUtils.fetchJson(plannedUrl)
                val currentRes = WidgetUtils.fetchJson(currentUrl)

                for (addr in addresses) {
                    totalCount += parseFortumItems(plannedRes, addr)
                    totalCount += parseFortumItems(currentRes, addr)
                }
            } catch (e: Exception) {
                continue
            }
        }
        return totalCount
    }

    private fun parseFortumItems(jsonString: String, settings: WidgetSettings): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("points") ?: return 0
        var count = 0
        val now = Date()
        val seenIds = mutableSetOf<String>()
        val isoFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.US)

        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val switchOffId = item.optString("switchOffId", "")
            if (switchOffId in seenIds) continue
            seenIds.add(switchOffId)

            val endDateStr = item.optString("endDate", "")
            if (!DateUtils.isOutageActive(endDateStr)) continue

            val message = item.optString("message", "")
            if (WidgetUtils.matchesStreetOnly(message, settings.streetName1, settings.streetName2)) count++
        }
        return count
    }
}
