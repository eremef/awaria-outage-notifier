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

    @JvmStatic
    external fun initVerifier(context: Context)

    init {
        try {
            System.loadLibrary("app_lib")
        } catch (e: Throwable) {
            println("Failed to load app_lib: ${e.message}")
        }
    }

    fun wordMatch(text: String, word: String): Boolean {
        if (text.isEmpty() || word.isEmpty()) return false
        val escapedWord = Pattern.quote(word)
        // Use manual boundaries [^\p{L}] instead of \b to better support Polish characters across Android versions
        val pattern = "(?ui)(?:^|[^\\p{L}0-9])$escapedWord(?:[^\\p{L}0-9]|$)"
        return Regex(pattern).containsMatchIn(text)
    }

    class CompiledMatcher(settings: WidgetSettings) {
        private val cityRegex = if (settings.cityName.isNotEmpty()) {
            Regex("(?ui)(?:^|[^\\p{L}0-9])${Pattern.quote(settings.cityName)}(?:[^\\p{L}0-9]|$)")
        } else null

        private val communeRegex = if (settings.commune.isNotEmpty()) {
            Regex("(?ui)(?:^|[^\\p{L}0-9])${Pattern.quote(settings.commune)}(?:[^\\p{L}0-9]|$)")
        } else null

        private val streetRegexes: List<Regex>

        init {
            val candidates = mutableListOf<String>()
            if (settings.streetName1.isNotEmpty()) {
                val s1 = settings.streetName1.trim()
                val s2 = settings.streetName2

                // 1. Compound name
                if (!s2.isNullOrEmpty() && s2 != "null") {
                    candidates.add("${s2.trim()} $s1")
                }

                // 2. Full streetName1
                if (!candidates.contains(s1)) {
                    candidates.add(s1)
                }
            }
            streetRegexes = candidates.map { Regex("(?ui)(?:^|[^\\p{L}0-9])${Pattern.quote(it)}(?:[^\\p{L}0-9]|$)") }
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

    @JvmStatic
    external fun fetchAndNotifyFromRust(context: Context, settingsJson: String)

    @JvmStatic
    fun showNotification(context: Context, title: String, body: String, hash: String) {
        val notificationManager = context.getSystemService(Context.NOTIFICATION_SERVICE) as android.app.NotificationManager
        
        // Create channel for Android 8.0+
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
            val channel = android.app.NotificationChannel(
                "outages",
                "Outage Alerts",
                android.app.NotificationManager.IMPORTANCE_DEFAULT
            )
            notificationManager.createNotificationChannel(channel)
        }

        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)
        val pendingIntent = android.app.PendingIntent.getActivity(
            context,
            hash.hashCode(),
            intent,
            android.app.PendingIntent.FLAG_UPDATE_CURRENT or android.app.PendingIntent.FLAG_IMMUTABLE
        )

        val builder = androidx.core.app.NotificationCompat.Builder(context, "outages")
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(androidx.core.app.NotificationCompat.BigTextStyle().bigText(body))
            .setPriority(androidx.core.app.NotificationCompat.PRIORITY_DEFAULT)
            .setContentIntent(pendingIntent)
            .setAutoCancel(true)

        notificationManager.notify(hash.hashCode(), builder.build())
    }

    @JvmStatic
    fun scheduleBackgroundMonitoring(context: android.content.Context) {
        val workManager = androidx.work.WorkManager.getInstance(context)
        val request = androidx.work.PeriodicWorkRequestBuilder<BackgroundMonitorWorker>(1, java.util.concurrent.TimeUnit.HOURS)
            .setConstraints(
                androidx.work.Constraints.Builder()
                    .setRequiredNetworkType(androidx.work.NetworkType.CONNECTED)
                    .build()
            )
            .build()
        
        workManager.enqueueUniquePeriodicWork(
            "BackgroundMonitorWork",
            androidx.work.ExistingPeriodicWorkPolicy.KEEP,
            request
        )
    }

    fun serializeSettingsForRust(settingsList: List<WidgetSettings>, fullJson: org.json.JSONObject?): String {
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
        
        // Extract language and preferences from fullJson
        if (fullJson != null) {
            root.put("language", fullJson.optString("language", "system"))
            root.put("notificationPreferences", fullJson.optJSONObject("notificationPreferences") ?: org.json.JSONObject())
            root.put("upcomingNotificationEnabled", fullJson.optBoolean("upcomingNotificationEnabled", false))
            root.put("upcomingNotificationHours", fullJson.optInt("upcomingNotificationHours", 24))
            root.put("enabledSources", fullJson.optJSONArray("enabledSources") ?: org.json.JSONArray())
        } else {
            root.put("upcomingNotificationEnabled", false)
            root.put("upcomingNotificationHours", 24)
        }
        
        return root.toString()
    }
}
