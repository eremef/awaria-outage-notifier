package xyz.eremef.awaria

import android.util.Log
import android.content.Context
import java.net.HttpURLConnection
import java.net.URL
import java.util.regex.Pattern

object WidgetUtils {
    private const val TAG = "AwariaWidgetUtils"

    @JvmStatic
    external fun fetchCountFromRust(context: Context, providerId: String, settingsJson: String): Int

    init {
        try {
            System.loadLibrary("app_lib")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to load app_lib: ${e.message}")
        }
    }

    fun wordMatch(text: String, word: String): Boolean {
        if (text.isEmpty() || word.isEmpty()) return false
        val escapedWord = Pattern.quote(word)
        // Use manual boundaries [^\p{L}] instead of \b to better support Polish characters across Android versions
        val pattern = "(?ui)(?:^|[^\\p{L}])$escapedWord(?:[^\\p{L}]|$)"
        return Regex(pattern).containsMatchIn(text)
    }

    class CompiledMatcher(settings: WidgetSettings) {
        private val cityRegex = if (settings.cityName.isNotEmpty()) {
            Regex("(?ui)(?:^|[^\\p{L}])${Pattern.quote(settings.cityName)}(?:[^\\p{L}]|$)")
        } else null

        private val communeRegex = if (settings.commune.isNotEmpty()) {
            Regex("(?ui)(?:^|[^\\p{L}])${Pattern.quote(settings.commune)}(?:[^\\p{L}]|$)")
        } else null

        private val streetRegexes: List<Regex>

        init {
            val candidates = mutableListOf<String>()
            if (settings.streetName1.isNotEmpty()) {
                // 1. Compound name
                val s2 = settings.streetName2
                if (!s2.isNullOrEmpty() && s2 != "null") {
                    candidates.add("${s2.trim()} ${settings.streetName1.trim()}")
                }

                // 2. Individual parts (min 3 chars)
                settings.streetName1.split(Regex("\\s+")).filter { it.length >= 3 }.forEach { candidates.add(it) }
                s2?.takeIf { it != "null" }?.split(Regex("\\s+"))?.filter { it.length >= 3 }?.forEach { candidates.add(it) }

                // 3. Full streetName (fallback)
                if (!candidates.contains(settings.streetName1)) candidates.add(settings.streetName1)
            }
            streetRegexes = candidates.map { Regex("(?ui)(?:^|[^\\p{L}])${Pattern.quote(it)}(?:[^\\p{L}]|$)") }
        }

        fun matchesCity(text: String): Boolean = cityRegex?.containsMatchIn(text) ?: true
        fun matchesCommune(text: String): Boolean = communeRegex?.containsMatchIn(text) ?: true
        
        fun matchesStreet(text: String): Boolean {
            if (streetRegexes.isEmpty()) return true
            return streetRegexes.any { it.containsMatchIn(text) }
        }

        /**
         * Full match logic for providers like Energa/Enea
         */
        fun matchesFull(message: String, areas: List<String>? = null): Boolean {
            if (!matchesCity(message)) return false
            
            // Check commune in message or areas
            val communeInMsg = matchesCommune(message)
            val communeInAreas = areas?.any { matchesCommune(it) } ?: false
            if (!communeInMsg && !communeInAreas) return false
            
            return matchesStreet(message)
        }
    }

    fun matchesStreetOnly(
        text: String,
        streetName1: String,
        streetName2: String?
    ): Boolean {
        if (text.isEmpty()) return false
        if (streetName1.isEmpty()) return true

        // For simple cases without a pre-compiled matcher
        val escaped1 = Pattern.quote(streetName1)
        if (Regex("(?ui)\\b$escaped1\\b").containsMatchIn(text)) return true
        
        streetName2?.takeIf { it != "null" }?.let { n2 ->
            val compound = Pattern.quote("$n2 $streetName1")
            if (Regex("(?ui)\\b$compound\\b").containsMatchIn(text)) return true
        }
        
        return false
    }

    fun fetchJson(url: URL, maxRetries: Int = 3): String {
        var lastException: Exception? = null
        var delay = 1000L

        for (attempt in 1..maxRetries) {
            try {
                return fetchJsonInternal(url)
            } catch (e: Exception) {
                lastException = e
                Log.w(TAG, "Fetch attempt $attempt failed for $url: ${e.message}")
                if (attempt < maxRetries) {
                    Thread.sleep(delay)
                    delay *= 2
                }
            }
        }
        throw lastException ?: Exception("Unknown fetch error")
    }

    private fun fetchJsonInternal(url: URL): String {
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

        val response = conn.inputStream.bufferedReader().use { it.readText() }
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

    fun serializeSettingsForRust(settingsList: List<WidgetSettings>): String {
        val root = org.json.JSONObject()
        val addresses = org.json.JSONArray()
        
        for (s in settingsList) {
            val addr = org.json.JSONObject()
            addr.put("name", s.name)
            addr.put("cityName", s.cityName)
            addr.put("voivodeship", s.voivodeship)
            addr.put("district", s.district)
            addr.put("commune", s.commune)
            addr.put("streetName", s.streetName)
            addr.put("streetName1", s.streetName1)
            addr.put("streetName2", if (s.streetName2 == null) org.json.JSONObject.NULL else s.streetName2)
            addr.put("houseNo", s.houseNo)
            addr.put("cityId", if (s.cityId == 0L) org.json.JSONObject.NULL else s.cityId)
            addr.put("streetId", if (s.streetId == 0L) org.json.JSONObject.NULL else s.streetId)
            addr.put("isActive", s.isActive)
            addresses.put(addr)
        }
        
        root.put("addresses", addresses)
        root.put("upcomingNotificationEnabled", false)
        root.put("upcomingNotificationHours", 24)
        
        return root.toString()
    }
}
