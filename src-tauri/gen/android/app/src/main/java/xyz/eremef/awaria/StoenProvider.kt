package xyz.eremef.awaria

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class StoenProvider : IOutageProvider {
    companion object {
        private const val TAG = "AwariaStoen"
    }

    override val id: String = "stoen"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { WidgetUtils.isWarszawa(it) }
        if (relevantSettings.isEmpty()) return 0

        var totalCount = 0
        try {
            val url =
                URL(
                    "https://awaria.stoen.pl/public/api/planned-outage/search/compressed-report"
                )
            val conn = url.openConnection() as HttpURLConnection
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/json")
            conn.setRequestProperty(
                "Referer",
                "https://awaria.stoen.pl/public/planned?pagelimit=9999"
            )
            conn.setRequestProperty("Origin", "https://awaria.stoen.pl")
            conn.connectTimeout = 15000
            conn.readTimeout = 15000

            val payload =
                JSONObject().apply {
                    put("id", null)
                    put("area", null)
                    put("outageStart", null)
                    put("outageEnd", null)
                    put(
                        "page",
                        JSONObject().apply {
                            put("limit", 9999)
                            put("offset", 0)
                        }
                    )
                }

            conn.outputStream.use { it.write(payload.toString().toByteArray(Charsets.UTF_8)) }

            if (conn.responseCode !in 200..299) {
                conn.disconnect()
                return 0
            }

            val response = conn.inputStream.bufferedReader().use { it.readText() }
            conn.disconnect()

            val outages = org.json.JSONArray(response)

            val matchers = relevantSettings.map { it to WidgetUtils.CompiledMatcher(it) }
            val counts = IntArray(relevantSettings.size) { 0 }

            for (i in 0 until outages.length()) {
                val outage = outages.getJSONObject(i)

                // Filter by date (End of outage must be in the future)
                val endStr = outage.optString("outageEnd", "")
                if (!DateUtils.isOutageActive(endStr, "yyyy-MM-dd HH:mm:ss")) continue

                val addresses = outage.optJSONArray("addresses") ?: continue
                
                for (adIdx in matchers.indices) {
                    val (settings, matcher) = matchers[adIdx]
                    if (settings.streetName1.isEmpty()) {
                        counts[adIdx]++
                        continue
                    }

                    var streetMatched = false
                    for (j in 0 until addresses.length()) {
                        val addr = addresses.getJSONObject(j)
                        val street = addr.optString("streetName", "")
                        if (matcher.matchesStreet(street)) {
                            streetMatched = true
                            break
                        }
                    }
                    if (streetMatched) counts[adIdx]++
                }
            }
            totalCount = counts.sum()
        } catch (e: Exception) {
            Log.e(TAG, "STOEN fetch error", e)
        }
        return totalCount
    }
}
