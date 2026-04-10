package xyz.eremef.awaria

import android.util.Log
import java.net.HttpURLConnection
import java.net.URL
import java.util.regex.Pattern

object WidgetUtils {
    private const val TAG = "AwariaWidgetUtils"

    fun wordMatch(text: String, word: String): Boolean {
        val escapedWord = Pattern.quote(word)
        val regex = Regex("\\b$escapedWord\\b", RegexOption.IGNORE_CASE)
        return regex.containsMatchIn(text)
    }

    fun matchesStreetOnly(
        text: String,
        streetName1: String,
        streetName2: String?
    ): Boolean {
        if (text.isEmpty()) return false

        // Village case: no street name
        if (streetName1.isEmpty()) {
            return true
        }

        // Compound name first (if streetName2 exists)
        if (streetName2 != null) {
            val compound = "$streetName2 $streetName1"
            if (wordMatch(text, compound)) return true
        }

        // Secondary: match main streetName1 as a whole word
        if (wordMatch(text, streetName1)) return true

        return false
    }

    fun fetchJson(url: URL): String {
        val conn = url.openConnection() as HttpURLConnection
        conn.requestMethod = "GET"
        conn.setRequestProperty("accept", "application/json")
        conn.connectTimeout = 10000
        conn.readTimeout = 10000

        val responseCode = conn.responseCode
        if (responseCode !in 200..299) {
            conn.disconnect()
            throw Exception("HTTP error: $responseCode at $url")
        }

        val response = conn.inputStream.bufferedReader().readText()
        conn.disconnect()
        return response
    }

    fun isWroclaw(settings: WidgetSettings): Boolean {
        val name = settings.cityName.lowercase()
        return name == "wrocław" || name == "wroclaw" || settings.cityId == 969400L
    }

    fun isWarszawa(settings: WidgetSettings): Boolean {
        val name = settings.cityName.lowercase()
        return name == "warszawa" || name == "warsaw" || settings.cityId == 918123L
    }
}
