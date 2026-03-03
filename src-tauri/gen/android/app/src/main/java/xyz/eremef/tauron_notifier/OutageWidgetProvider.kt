package xyz.eremef.tauron_notifier

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.widget.RemoteViews
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import org.json.JSONObject
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.ExistingPeriodicWorkPolicy
import java.util.concurrent.TimeUnit

data class WidgetSettings(
    val cityGAID: Long,
    val streetGAID: Long,
    val houseNo: String,
    val streetName: String,
    val theme: String,
    val language: String
)

class OutageWidgetProvider : AppWidgetProvider() {

    companion object {
        private const val ACTION_REFRESH = "xyz.eremef.tauron_notifier.ACTION_REFRESH"
        private const val WORK_NAME = "xyz.eremef.tauron_notifier.WIDGET_UPDATE_WORK"

        // Light theme colors (from style.css :root)
        private const val LIGHT_PRIMARY = "#D9006C"
        private const val LIGHT_LABEL = "#666666"
        private const val LIGHT_UPDATED = "#999999"

        // Dark theme colors (from style.css [data-theme="dark"])
        private const val DARK_PRIMARY = "#FF4DA6"
        private const val DARK_LABEL = "#A0A0A0"
        private const val DARK_UPDATED = "#777777"
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == ACTION_REFRESH ||
            intent.action == Intent.ACTION_BOOT_COMPLETED) {
            val mgr = AppWidgetManager.getInstance(context)
            val ids = mgr.getAppWidgetIds(
                ComponentName(context, OutageWidgetProvider::class.java)
            )
            onUpdate(context, mgr, ids)
        }
        super.onReceive(context, intent)
    }

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        scheduleWork(context)
        val pendingResult = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                for (appWidgetId in appWidgetIds) {
                    updateWidget(context, appWidgetManager, appWidgetId)
                }
            } finally {
                pendingResult.finish()
            }
        }
    }

    override fun onEnabled(context: Context) {
        super.onEnabled(context)
        scheduleWork(context)
    }

    override fun onDisabled(context: Context) {
        super.onDisabled(context)
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
    }

    private fun scheduleWork(context: Context) {
        val request = PeriodicWorkRequestBuilder<WidgetUpdateWorker>(1, TimeUnit.HOURS)
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(
            WORK_NAME,
            ExistingPeriodicWorkPolicy.KEEP,
            request
        )
    }

    private fun findSettingsFile(context: Context): File? {
        // Build a list of candidate directories where Tauri might store settings.json
        val candidates = mutableListOf<File>()

        // 1. context.filesDir  →  /data/.../files/
        candidates.add(File(context.filesDir, "settings.json"))

        // 2. context.dataDir  →  /data/.../
        candidates.add(File(context.dataDir, "settings.json"))

        // 3. app_data subdir  →  /data/.../app_data/  (Tauri's typical app_data_dir)
        context.filesDir.parentFile?.let { parent ->
            candidates.add(File(parent, "app_data/settings.json"))
        }

        // 4. Walk one level inside dataDir for any subfolder containing settings.json
        context.dataDir.listFiles()?.filter { it.isDirectory }?.forEach { dir ->
            candidates.add(File(dir, "settings.json"))
        }

        return candidates.firstOrNull { it.exists() && it.canRead() }
    }

    private fun loadSettings(context: Context): WidgetSettings? {
        val settingsFile = findSettingsFile(context) ?: return null

        return try {
            parseSettings(settingsFile.readText())
        } catch (e: Exception) {
            null
        }
    }

    internal fun parseSettings(jsonString: String): WidgetSettings? {
        return try {
            val json = JSONObject(jsonString)
            WidgetSettings(
                cityGAID = json.getLong("cityGAID"),
                streetGAID = json.getLong("streetGAID"),
                houseNo = json.getString("houseNo"),
                streetName = json.getString("streetName"),
                theme = json.optString("theme", "system"),
                language = json.optString("language", "system")
            )
        } catch (e: Exception) {
            null
        }
    }

    private fun isDarkMode(context: Context, themeSetting: String): Boolean {
        return when (themeSetting) {
            "dark" -> true
            "light" -> false
            else -> {
                // "system" or missing — follow Android system setting
                val nightMode = context.resources.configuration.uiMode and
                        Configuration.UI_MODE_NIGHT_MASK
                nightMode == Configuration.UI_MODE_NIGHT_YES
            }
        }
    }

    private fun applyTheme(views: RemoteViews, dark: Boolean) {
        if (dark) {
            views.setInt(R.id.widget_root, "setBackgroundResource", R.drawable.widget_background_dark)
            views.setTextColor(R.id.widget_count, Color.parseColor(DARK_PRIMARY))
            views.setTextColor(R.id.widget_label, Color.parseColor(DARK_LABEL))
            views.setTextColor(R.id.widget_updated, Color.parseColor(DARK_UPDATED))
        } else {
            views.setInt(R.id.widget_root, "setBackgroundResource", R.drawable.widget_background)
            views.setTextColor(R.id.widget_count, Color.parseColor(LIGHT_PRIMARY))
            views.setTextColor(R.id.widget_label, Color.parseColor(LIGHT_LABEL))
            views.setTextColor(R.id.widget_updated, Color.parseColor(LIGHT_UPDATED))
        }
    }

    private fun getTranslation(key: String, lang: String): String {
        val isPl = if (lang == "pl") {
            true
        } else if (lang == "en") {
            false
        } else {
            val systemLang = Locale.getDefault().language
            systemLang.startsWith("pl")
        }
        return when (key) {
            "outages" -> if (isPl) "wyłączeń" else "outages"
            "setup" -> if (isPl) "Skonfiguruj" else "Setup needed"
            "updating" -> if (isPl) "Aktualizacja..." else "Updating..."
            else -> key
        }
    }

    internal fun updateWidget(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int
    ) {
        val views = RemoteViews(context.packageName, R.layout.widget_outage)

        // Set tap-to-refresh intent
        val refreshIntent = Intent(context, OutageWidgetProvider::class.java).apply {
            action = ACTION_REFRESH
        }
        val refreshPending = PendingIntent.getBroadcast(
            context, 0, refreshIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        views.setOnClickPendingIntent(R.id.widget_root, refreshPending)

        // Load settings
        val settings = loadSettings(context)

        // Apply theme (even before settings are fully loaded)
        val dark = isDarkMode(context, settings?.theme ?: "system")
        applyTheme(views, dark)

        val language = settings?.language ?: "system"
        views.setTextViewText(R.id.widget_label, getTranslation("outages", language))

        if (settings == null) {
            views.setTextViewText(R.id.widget_count, "?")
            views.setTextViewText(R.id.widget_updated, getTranslation("setup", language))
            appWidgetManager.updateAppWidget(appWidgetId, views)
            return
        }

        // Show loading state
        views.setTextViewText(R.id.widget_count, "…")
        views.setTextViewText(R.id.widget_updated, getTranslation("updating", language))
        appWidgetManager.updateAppWidget(appWidgetId, views)

        // Fetch data
        try {
            val count = fetchFilteredOutageCount(settings)
            val timeFormat = SimpleDateFormat("HH:mm", Locale.getDefault())
            val updatedAt = timeFormat.format(Date())

            views.setTextViewText(R.id.widget_count, count.toString())
            views.setTextViewText(R.id.widget_updated, updatedAt)
        } catch (e: Exception) {
            views.setTextViewText(R.id.widget_count, "!")
            val errMsg = (e.message ?: "Unknown").take(20)
            views.setTextViewText(R.id.widget_updated, errMsg)
        }
        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    private fun fetchFilteredOutageCount(settings: WidgetSettings): Int {
        val dateFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
        dateFormat.timeZone = TimeZone.getTimeZone("UTC")
        val now = dateFormat.format(Date())
        val baseUrl = "https://www.tauron-dystrybucja.pl/waapi/outages/address"
        val params = "cityGAID=${settings.cityGAID}&streetGAID=${settings.streetGAID}" +
                "&houseNo=${settings.houseNo}" +
                "&fromDate=$now&getLightingSupport=false" +
                "&getServicedSwitchingoff=true&_=${System.currentTimeMillis()}"

        val url = URL("$baseUrl?$params")
        val conn = url.openConnection() as HttpURLConnection
        conn.requestMethod = "GET"
        conn.setRequestProperty("accept", "application/json")
        conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
        conn.setRequestProperty("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        conn.connectTimeout = 10000
        conn.readTimeout = 10000

        val responseCode = conn.responseCode
        if (responseCode !in 200..299) {
            conn.disconnect()
            throw Exception("HTTP error: $responseCode")
        }

        val response = conn.inputStream.bufferedReader().readText()
        conn.disconnect()

        return parseOutageItems(response, settings.streetName)
    }

    internal fun parseOutageItems(jsonString: String, streetName: String): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("OutageItems") ?: return 0

        // Normalize street name
        // Remove "ul.", "al.", "pl.", "os.", "rondo" (case insensitive)
        val normalizeRegex = Regex("(?i)^(ul\\.|al\\.|pl\\.|os\\.|rondo)\\s*")
        val fullStreet = normalizeRegex.replace(streetName, "").trim()
        
        if (fullStreet.isEmpty()) return 0

        // Significant words
        val significantWords = fullStreet.split(Regex("\\s+"))
            .filter { it.length >= 3 }

        var count = 0
        val now = Date()
        // Try parsing different common formats returned by the API
        val formats = listOf(
            SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") },
            SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS", Locale.US),
            SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.US)
        )

        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            
            // 0. Filter out finished outages
            val endDateStr = item.optString("EndDate", "")
            if (endDateStr.isNotEmpty()) {
                var parsedDate: Date? = null
                for (fmt in formats) {
                    try {
                        parsedDate = fmt.parse(endDateStr)
                        if (parsedDate != null) break
                    } catch (e: Exception) {}
                }
                if (parsedDate != null && parsedDate.before(now)) {
                    continue
                }
            }

            val message = item.optString("Message", "")
            
            // 1. Check full street name
            if (message.contains(streetName)) {
                count++
                continue
            }

            // 2. Check significant words with word boundaries
            val anyMatch = significantWords.any { word ->
                val escapedWord = java.util.regex.Pattern.quote(word)
                // Use word boundaries \b
                val regex = Regex("\\b$escapedWord\\b")
                regex.containsMatchIn(message)
            }

            if (anyMatch) {
                count++
            }
        }
        return count
    }
}
