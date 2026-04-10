package xyz.eremef.awaria

import android.content.Context
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class MpwikProvider : IOutageProvider {
    override val id: String = "water"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { WidgetUtils.isWroclaw(it) }
        if (relevantSettings.isEmpty()) return 0

        val url = URL("https://www.mpwik.wroc.pl/wp-admin/admin-ajax.php")
        val conn = url.openConnection() as HttpURLConnection
        conn.requestMethod = "POST"
        conn.doOutput = true
        conn.setRequestProperty("content-type", "application/x-www-form-urlencoded; charset=UTF-8")
        conn.setRequestProperty("accept", "application/json")
        conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
        conn.setRequestProperty("origin", "https://www.mpwik.wroc.pl")
        conn.setRequestProperty("referer", "https://www.mpwik.wroc.pl/")
        conn.connectTimeout = 10000
        conn.readTimeout = 10000

        val postData = "action=all"
        conn.outputStream.write(postData.toByteArray(Charsets.UTF_8))

        val responseCode = conn.responseCode
        if (responseCode !in 200..299) {
            conn.disconnect()
            throw Exception("MPWiK HTTP error: $responseCode")
        }

        val response = conn.inputStream.bufferedReader().readText()
        conn.disconnect()

        var totalCount = 0
        for (settings in relevantSettings) {
            totalCount += parseMpwikItems(response, settings)
        }
        return totalCount
    }

    private fun parseMpwikItems(jsonString: String, settings: WidgetSettings): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("failures") ?: return 0
        var count = 0
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val endDateStr = item.optString("date_end", "")
            if (!DateUtils.isOutageActive(endDateStr)) continue
            val content = item.optString("content", "")
            if (WidgetUtils.matchesStreetOnly(content, settings.streetName1, settings.streetName2)) count++
        }
        return count
    }
}
