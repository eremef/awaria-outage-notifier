package xyz.eremef.awaria

import android.util.Log
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import java.net.HttpURLConnection
import java.net.URL
import java.util.regex.Pattern

object WidgetUtils {
    private const val TAG = "AwariaWidgetUtils"
    
    @JvmStatic
    fun isNetworkAvailable(context: Context): Boolean {
        return try {
            val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                val nw = cm.activeNetwork ?: return false
                val actNw = cm.getNetworkCapabilities(nw) ?: return false
                when {
                    actNw.hasTransport(android.net.NetworkCapabilities.TRANSPORT_WIFI) -> true
                    actNw.hasTransport(android.net.NetworkCapabilities.TRANSPORT_CELLULAR) -> true
                    actNw.hasTransport(android.net.NetworkCapabilities.TRANSPORT_ETHERNET) -> true
                    else -> false
                }
            } else {
                @Suppress("DEPRECATION")
                val nwInfo = cm.activeNetworkInfo ?: return false
                nwInfo.isConnected
            }
        } catch (e: Exception) {
            Log.w(TAG, "isNetworkAvailable failed, assuming online: ${e.message}")
            true // fail open — let the fetch attempt proceed
        }
    }

    @JvmStatic
    fun readUri(context: Context, uriString: String): String {
        return try {
            val uri = Uri.parse(uriString)
            context.contentResolver.openInputStream(uri)?.use { input ->
                input.bufferedReader().use { it.readText() }
            } ?: ""
        } catch (e: Exception) {
            Log.e(TAG, "Failed to read URI: ${e.message}", e)
            ""
        }
    }

    @JvmStatic
    fun exportSettings(context: Context, filePath: String, fileName: String): String {
        try {
            val sourceFile = java.io.File(filePath)
            if (!sourceFile.exists()) return "Source file not found"

            val resolver = context.contentResolver
            val contentValues = android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, fileName)
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "application/json")
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    put(android.provider.MediaStore.MediaColumns.RELATIVE_PATH, android.os.Environment.DIRECTORY_DOWNLOADS + "/Awaria")
                    put(android.provider.MediaStore.MediaColumns.IS_PENDING, 1)
                }
            }

            val collection = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI
            } else {
                // Fallback for older versions - we'll just use the share sheet
                // as direct writing to Downloads without SAF/permissions is restricted
                shareFile(context, filePath, "application/json", "Export Settings")
                return "Exporting via share sheet..."
            }

            val uri = resolver.insert(collection, contentValues)
            if (uri != null) {
                resolver.openOutputStream(uri)?.use { output ->
                    sourceFile.inputStream().use { input ->
                        input.copyTo(output)
                    }
                }
                
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    contentValues.clear()
                    contentValues.put(android.provider.MediaStore.MediaColumns.IS_PENDING, 0)
                    resolver.update(uri, contentValues, null, null)
                }
                
                return "Saved to Downloads/Awaria/$fileName"
            } else {
                // Fallback to share sheet if MediaStore fails
                shareFile(context, filePath, "application/json", "Export Settings")
                return "Exporting via share sheet..."
            }
        } catch (e: Exception) {
            Log.e(TAG, "Export failed: ${e.message}", e)
            // Final fallback
            shareFile(context, filePath, "application/json", "Export Settings")
            return "Exporting via share sheet..."
        }
    }

    @JvmStatic
    fun shareFile(context: Context, filePath: String, mimeType: String, title: String) {
        try {
            val file = java.io.File(filePath)
            if (!file.exists()) {
                Log.e(TAG, "File not found: $filePath")
                return
            }

            val uri: Uri = androidx.core.content.FileProvider.getUriForFile(
                context,
                "${context.packageName}.fileprovider",
                file
            )

            val intent = Intent(Intent.ACTION_SEND)
            // Use */* or application/octet-stream to ensure more apps handle it if application/json fails
            intent.type = if (mimeType == "application/json") "text/plain" else mimeType
            intent.putExtra(Intent.EXTRA_STREAM, uri)
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            
            val chooser = Intent.createChooser(intent, title)
            chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(chooser)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to share file: ${e.message}", e)
        }
    }

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

    @JvmStatic
    fun isIgnoringBatteryOptimizations(context: Context): Boolean {
        // Check for "Restricted" background status on Android 9+
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val am = context.getSystemService(Context.ACTIVITY_SERVICE) as android.app.ActivityManager
            if (am.isBackgroundRestricted) {
                return false
            }
        }
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
            return pm.isIgnoringBatteryOptimizations(context.packageName)
        }
        return true
    }

    @JvmStatic
    fun requestIgnoreBatteryOptimizations(context: Context) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
            val packageName = context.packageName
            
            if (!pm.isIgnoringBatteryOptimizations(packageName)) {
                android.util.Log.i("AwariaBgMonitor", "Requesting battery optimization exclusion (polite way)")
                val intent = Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)
                intent.flags = Intent.FLAG_ACTIVITY_NEW_TASK
                context.startActivity(intent)
            }
        }
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
            root.put("filterByHouseNo", fullJson.optBoolean("filterByHouseNo", false))
        } else {
            root.put("upcomingNotificationEnabled", false)
            root.put("upcomingNotificationHours", 24)
            root.put("filterByHouseNo", false)
        }
        
        return root.toString()
    }
}
