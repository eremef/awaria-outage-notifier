package xyz.eremef.awaria

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.os.Build
import android.util.SizeF
import android.widget.RemoteViews
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONObject

data class WidgetSettings(
        val cityGAID: Long,
        val streetGAID: Long,
        val houseNo: String,
        val streetName: String,
        val theme: String,
        val language: String
)

abstract class BaseWidgetProvider : AppWidgetProvider() {

    companion object {
        const val WORK_NAME = "xyz.eremef.awaria.WIDGET_UPDATE_WORK"

        // Light theme colors (from style.css :root)
        private const val LIGHT_PRIMARY = "#D9006C"
        private const val LIGHT_LABEL = "#666666"
        private const val LIGHT_UPDATED = "#999999"

        // Dark theme colors (from style.css [data-theme="dark"])
        private const val DARK_PRIMARY = "#FF4DA6"
        private const val DARK_LABEL = "#A0A0A0"
        private const val DARK_UPDATED = "#777777"
    }

    abstract val refreshAction: String
    abstract val lightPrimary: String
    abstract val darkPrimary: String
    abstract val iconResId: Int

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == refreshAction || intent.action == Intent.ACTION_BOOT_COMPLETED) {
            val mgr = AppWidgetManager.getInstance(context)
            val ids = mgr.getAppWidgetIds(ComponentName(context, this::class.java))
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

    override fun onAppWidgetOptionsChanged(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int,
            newOptions: android.os.Bundle
    ) {
        super.onAppWidgetOptionsChanged(context, appWidgetManager, appWidgetId, newOptions)
        val pendingResult = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                updateWidget(context, appWidgetManager, appWidgetId)
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
        // Only cancel if no other widgets are active?
        // For simplicity, we keep it running as long as any widget might need it.
        // Actually, WorkManager with periodic work is fine to leave or we can be smart.
    }

    private fun scheduleWork(context: Context) {
        val request = PeriodicWorkRequestBuilder<WidgetUpdateWorker>(1, TimeUnit.HOURS).build()
        WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    private fun findSettingsFile(context: Context): File? {
        val candidates = mutableListOf<File>()
        candidates.add(File(context.filesDir, "settings.json"))
        candidates.add(File(context.dataDir, "settings.json"))
        context.filesDir.parentFile?.let { parent ->
            candidates.add(File(parent, "app_data/settings.json"))
        }
        context.dataDir.listFiles()?.filter { it.isDirectory }?.forEach { dir ->
            candidates.add(File(dir, "settings.json"))
        }
        return candidates.firstOrNull { it.exists() && it.canRead() }
    }

    private fun loadSettings(context: Context): WidgetSettings? {
        val settingsFile = findSettingsFile(context) ?: return null
        return try {
            val json = JSONObject(settingsFile.readText())
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
                val nightMode =
                        context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
                nightMode == Configuration.UI_MODE_NIGHT_YES
            }
        }
    }

    private fun applyTheme(views: RemoteViews, dark: Boolean) {
        if (dark) {
            views.setInt(
                    R.id.widget_root,
                    "setBackgroundResource",
                    R.drawable.widget_background_dark
            )
            views.setTextColor(R.id.widget_count, Color.parseColor(darkPrimary))
            views.setTextColor(R.id.widget_label, Color.parseColor(DARK_LABEL))
            views.setTextColor(R.id.widget_updated, Color.parseColor(DARK_UPDATED))
            views.setInt(R.id.widget_icon, "setColorFilter", Color.parseColor(darkPrimary))
        } else {
            views.setInt(R.id.widget_root, "setBackgroundResource", R.drawable.widget_background)
            views.setTextColor(R.id.widget_count, Color.parseColor(lightPrimary))
            views.setTextColor(R.id.widget_label, Color.parseColor(LIGHT_LABEL))
            views.setTextColor(R.id.widget_updated, Color.parseColor(LIGHT_UPDATED))
            views.setInt(R.id.widget_icon, "setColorFilter", Color.parseColor(lightPrimary))
        }
        views.setImageViewResource(R.id.widget_icon, iconResId)
    }

    private fun getTranslation(key: String, lang: String): String {
        val isPl =
                if (lang == "pl") true
                else if (lang == "en") false else Locale.getDefault().language.startsWith("pl")
        return when (key) {
            "outages" -> if (isPl) "wyłączeń" else "outages"
            "alerts" -> if (isPl) "alertów" else "alerts"
            "setup" -> if (isPl) "Skonfiguruj" else "Setup needed"
            "updating" -> if (isPl) "Aktualizacja..." else "Updating..."
            else -> key
        }
    }

    abstract suspend fun fetchCount(settings: WidgetSettings): Int

    internal suspend fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
    ) {
        val settings = loadSettings(context)
        val language = settings?.language ?: "system"
        val dark = isDarkMode(context, settings?.theme ?: "system")

        val count =
                try {
                    if (settings != null) fetchCount(settings).toString() else "?"
                } catch (e: Exception) {
                    "!"
                }

        val updatedAt =
                if (count != "?" && count != "!") {
                    val timeFormat = SimpleDateFormat("HH:mm", Locale.getDefault())
                    timeFormat.format(Date())
                } else if (settings == null) {
                    getTranslation("setup", language)
                } else {
                    "Error"
                }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val viewMapping =
                    mapOf(
                            SizeF(40f, 40f) to
                                    createRemoteViews(
                                            context,
                                            R.layout.widget_outage_small,
                                            count,
                                            updatedAt,
                                            language,
                                            dark
                                    ),
                            SizeF(100f, 100f) to
                                    createRemoteViews(
                                            context,
                                            R.layout.widget_outage,
                                            count,
                                            updatedAt,
                                            language,
                                            dark
                                    ),
                            SizeF(200f, 200f) to
                                    createRemoteViews(
                                            context,
                                            R.layout.widget_outage_large,
                                            count,
                                            updatedAt,
                                            language,
                                            dark
                                    )
                    )
            val views = RemoteViews(viewMapping)
            appWidgetManager.updateAppWidget(appWidgetId, views)
        } else {
            // Legacy: pick best layout based on current options
            val options = appWidgetManager.getAppWidgetOptions(appWidgetId)
            val minWidth = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
            val minHeight = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT)

            val layoutId =
                    if (minWidth < 100 || minHeight < 100) {
                        R.layout.widget_outage_small
                    } else if (minWidth < 200 || minHeight < 200) {
                        R.layout.widget_outage
                    } else {
                        R.layout.widget_outage_large
                    }

            val views = createRemoteViews(context, layoutId, count, updatedAt, language, dark)
            appWidgetManager.updateAppWidget(appWidgetId, views)
        }
    }

    private fun createRemoteViews(
            context: Context,
            layoutId: Int,
            count: String,
            updatedAt: String,
            language: String,
            dark: Boolean
    ): RemoteViews {
        val views = RemoteViews(context.packageName, layoutId)

        val refreshIntent = Intent(context, this::class.java).apply { action = refreshAction }
        val refreshPending =
                PendingIntent.getBroadcast(
                        context,
                        0,
                        refreshIntent,
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                )

        val clickPending =
                if (count == "0") {
                    refreshPending
                } else {
                    val launchIntent =
                            context.packageManager.getLaunchIntentForPackage(context.packageName)
                                    ?.apply {
                                        flags =
                                                Intent.FLAG_ACTIVITY_NEW_TASK or
                                                        Intent.FLAG_ACTIVITY_CLEAR_TOP
                                    }
                    if (launchIntent != null) {
                        val activityPending =
                                PendingIntent.getActivity(
                                        context,
                                        0,
                                        launchIntent,
                                        PendingIntent.FLAG_UPDATE_CURRENT or
                                                PendingIntent.FLAG_IMMUTABLE
                                )
                        refreshPending.send()
                        activityPending
                    } else {
                        refreshPending
                    }
                }
        views.setOnClickPendingIntent(R.id.widget_root, clickPending)

        applyTheme(views, dark)
        views.setTextViewText(R.id.widget_label, getTranslation("alerts", language))
        views.setTextViewText(R.id.widget_count, count)
        views.setTextViewText(R.id.widget_updated, updatedAt)

        return views
    }

    protected fun fetchTauronAlertCount(settings: WidgetSettings): Int {
        val dateFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
        dateFormat.timeZone = TimeZone.getTimeZone("UTC")
        val now = dateFormat.format(Date())
        val baseUrl = "https://www.tauron-dystrybucja.pl/waapi/outages/address"
        val params =
                "cityGAID=${settings.cityGAID}&streetGAID=${settings.streetGAID}" +
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
            throw Exception("Tauron HTTP error: $responseCode")
        }

        val response = conn.inputStream.bufferedReader().readText()
        conn.disconnect()

        return parseOutageItems(response, settings.streetName)
    }

    protected fun fetchMpwikAlertCount(settings: WidgetSettings): Int {
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

        return parseMpwikItems(response, settings.streetName)
    }

    protected fun fetchFortumAlertCount(settings: WidgetSettings): Int {
        val cityGuid = "d06e8606-f1d7-eb11-bacb-000d3aa9626e"
        val regionId = "3"

        val plannedUrl =
                URL(
                        "https://formularz.fortum.pl/api/v1/switchoffs?cityGuid=$cityGuid&regionId=$regionId&current=false"
                )
        val currentUrl =
                URL(
                        "https://formularz.fortum.pl/api/v1/switchoffs?cityGuid=$cityGuid&regionId=$regionId&current=true"
                )

        val plannedResponse = fetchJson(plannedUrl)
        val currentResponse = fetchJson(currentUrl)

        val plannedCount = parseFortumItems(plannedResponse, settings.streetName)
        val currentCount = parseFortumItems(currentResponse, settings.streetName)

        return plannedCount + currentCount
    }

    private fun fetchJson(url: URL): String {
        val conn = url.openConnection() as HttpURLConnection
        conn.requestMethod = "GET"
        conn.setRequestProperty("accept", "application/json")
        conn.connectTimeout = 10000
        conn.readTimeout = 10000

        val responseCode = conn.responseCode
        if (responseCode !in 200..299) {
            conn.disconnect()
            throw Exception("Fortum HTTP error: $responseCode")
        }

        val response = conn.inputStream.bufferedReader().readText()
        conn.disconnect()
        return response
    }

    internal fun parseFortumItems(jsonString: String, streetName: String): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("points") ?: return 0
        val normalizeRegex = Regex("(?i)^(ul\\.|al\\.|pl\\.|os\\.|rondo)\\s*")
        val fullStreet = normalizeRegex.replace(streetName, "").trim()
        if (fullStreet.isEmpty()) return 0
        val significantWords = fullStreet.split(Regex("\\s+")).filter { it.length >= 3 }
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
            if (endDateStr.isNotEmpty()) {
                try {
                    val end = isoFormat.parse(endDateStr)
                    if (end != null && end.before(now)) continue
                } catch (e: Exception) {}
            }

            val message = item.optString("message", "")
            if (message.contains(streetName)) {
                count++
                continue
            }
            val anyMatch =
                    significantWords.any { word ->
                        val escapedWord = java.util.regex.Pattern.quote(word)
                        val regex = Regex("\\b$escapedWord\\b")
                        regex.containsMatchIn(message)
                    }
            if (anyMatch) count++
        }
        return count
    }

    internal fun parseMpwikItems(jsonString: String, streetName: String): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("failures") ?: return 0
        val normalizeRegex = Regex("(?i)^(ul\\.|al\\.|pl\\.|os\\.|rondo)\\s*")
        val fullStreet = normalizeRegex.replace(streetName, "").trim()
        if (fullStreet.isEmpty()) return 0
        val significantWords = fullStreet.split(Regex("\\s+")).filter { it.length >= 3 }
        var count = 0
        val now = Date()
        val mpwikFormat = SimpleDateFormat("dd-MM-yyyy HH:mm", Locale.getDefault())
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val endDateStr = item.optString("date_end", "")
            if (endDateStr.isNotEmpty()) {
                try {
                    val end = mpwikFormat.parse(endDateStr)
                    if (end != null && end.before(now)) continue
                } catch (e: Exception) {}
            }
            val content = item.optString("content", "")
            if (content.contains(streetName)) {
                count++
                continue
            }
            val anyMatch =
                    significantWords.any { word ->
                        val escapedWord = java.util.regex.Pattern.quote(word)
                        val regex = Regex("\\b$escapedWord\\b")
                        regex.containsMatchIn(content)
                    }
            if (anyMatch) count++
        }
        return count
    }

    internal fun parseOutageItems(jsonString: String, streetName: String): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("OutageItems") ?: return 0
        val normalizeRegex = Regex("(?i)^(ul\\.|al\\.|pl\\.|os\\.|rondo)\\s*")
        val fullStreet = normalizeRegex.replace(streetName, "").trim()
        if (fullStreet.isEmpty()) return 0
        val significantWords = fullStreet.split(Regex("\\s+")).filter { it.length >= 3 }
        var count = 0
        val now = Date()
        val formats =
                listOf(
                        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US).apply {
                            timeZone = TimeZone.getTimeZone("UTC")
                        },
                        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS", Locale.US),
                        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.US)
                )
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val endDateStr = item.optString("EndDate", "")
            if (endDateStr.isNotEmpty()) {
                var parsedDate: Date? = null
                for (fmt in formats) {
                    try {
                        parsedDate = fmt.parse(endDateStr)
                        if (parsedDate != null) break
                    } catch (e: Exception) {}
                }
                if (parsedDate != null && parsedDate.before(now)) continue
            }
            val message = item.optString("Message", "")
            if (message.contains(streetName)) {
                count++
                continue
            }
            val anyMatch =
                    significantWords.any { word ->
                        val escapedWord = java.util.regex.Pattern.quote(word)
                        val regex = Regex("\\b$escapedWord\\b")
                        regex.containsMatchIn(message)
                    }
            if (anyMatch) count++
        }
        return count
    }
}
