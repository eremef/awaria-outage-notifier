package xyz.eremef.awaria

import android.content.Context
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

class MpwikProvider : IOutageProvider {
    override val id: String = "water"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        // Only fetch for Wrocław addresses
        val relevantSettings = settingsList.filter { WidgetUtils.isWroclaw(it) }
        if (relevantSettings.isEmpty()) return 0

        val url = URL("https://www.mpwik.wroc.pl/wp-admin/admin-ajax.php")
        val conn = url.openConnection() as HttpURLConnection
        conn.requestMethod = "POST"
        conn.doOutput = true
        conn.setRequestProperty("content-type", "application/x-www-form-urlencoded; charset=UTF-8")
        conn.setRequestProperty("accept", "application/json")
        conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
        conn.setRequestProperty("user-agent", "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Mobile Safari/537.36")
        conn.setRequestProperty("referer", "https://www.mpwik.wroc.pl/")
        conn.connectTimeout = 15000
        conn.readTimeout = 15000

        val postData = "action=all"
        conn.outputStream.use { it.write(postData.toByteArray(Charsets.UTF_8)) }

        val responseCode = conn.responseCode
        if (responseCode !in 200..299) {
            conn.disconnect()
            throw Exception("MPWiK HTTP error: $responseCode")
        }

        val response = conn.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
        conn.disconnect()

        var totalCount = 0
        for (settings in relevantSettings) {
            try {
                totalCount += parseMpwikItems(response, settings)
            } catch (e: Exception) {
                android.util.Log.e("MpwikProvider", "JSON parse error for ${settings.streetName}: ${e.message}")
                throw e // Make it error out properly so we see "!" instead of wrong count
            }
        }
        return totalCount
    }

    private fun parseMpwikItems(jsonString: String, settings: WidgetSettings): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("failures") ?: return 0
        
        val matcher = WidgetUtils.CompiledMatcher(settings)
        var count = 0
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val endDateStr = item.optString("date_end", "")
            
            // MPWiK format: dd-MM-yyyy HH:mm
            if (!DateUtils.isOutageActive(endDateStr, "dd-MM-yyyy HH:mm")) continue
            
            val content = item.optString("content", "")
            if (matcher.matchesStreet(content)) {
                count++
            }
        }
        return count
    }
}
