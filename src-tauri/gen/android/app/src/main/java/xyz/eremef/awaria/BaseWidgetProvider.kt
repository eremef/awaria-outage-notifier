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
import android.util.Log
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
import kotlinx.coroutines.*
import org.json.JSONObject

data class WidgetSettings(
        val cityName: String,
        val voivodeship: String,
        val district: String,
        val commune: String,
        val streetName: String,
        val streetName1: String,
        val streetName2: String?,
        val houseNo: String,
        val cityId: Long,
        val streetId: Long,
        val theme: String,
        val language: String,
        val isActive: Boolean,
        val sourceEnabled: Boolean
)

abstract class BaseWidgetProvider : AppWidgetProvider() {

    companion object {
        const val WORK_NAME = "xyz.eremef.awaria.WIDGET_UPDATE_WORK"
        const val TAG = "AwariaWidget"

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
    abstract val labelKey: String
    abstract val sourceKey: String

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

    private fun loadSettings(context: Context): List<WidgetSettings>? {
        val settingsFile = findSettingsFile(context) ?: return null
        return try {
            val json = JSONObject(settingsFile.readText())
            val addresses = json.optJSONArray("addresses")
            val enabledSources = json.optJSONArray("enabledSources")

            val isSourceEnabled =
                    if (enabledSources != null) {
                        var found = false
                        for (i in 0 until enabledSources.length()) {
                            if (enabledSources.getString(i) == sourceKey) {
                                found = true
                                break
                            }
                        }
                        found
                    } else {
                        true // Default to enabled if field missing
                    }

            if (addresses != null && addresses.length() > 0) {
                (0 until addresses.length()).map { i ->
                    val addr = addresses.getJSONObject(i)
                    WidgetSettings(
                            cityName = addr.optString("cityName", ""),
                            voivodeship = addr.optString("voivodeship", ""),
                            district = addr.optString("district", ""),
                            commune = addr.optString("commune", ""),
                            streetName = addr.optString("streetName", ""),
                            streetName1 = addr.optString("streetName1", ""),
                            streetName2 =
                                    addr.optString("streetName2", "").let {
                                        if (it.isEmpty()) null else it
                                    },
                            houseNo = addr.optString("houseNo", ""),
                            cityId = addr.optLong("cityId", 0),
                            streetId = addr.optLong("streetId", 0),
                            theme = json.optString("theme", "system"),
                            language = json.optString("language", "system"),
                            isActive = addr.optBoolean("isActive", true),
                            sourceEnabled = isSourceEnabled
                    )
                }
            } else {
                null
            }
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
        views.setTextViewText(R.id.widget_source, getSourceName(sourceKey))
    }

    private fun getSourceName(key: String): String {
        return when (key) {
            "tauron" -> "Tauron"
            "stoen" -> "Stoen"
            "enea" -> "Enea"
            "energa" -> "Energa"
            "pge" -> "PGE"
            "fortum" -> "Fortum"
            "water" -> "MPWiK"
            else ->
                    key.replaceFirstChar {
                        if (it.isLowerCase()) it.titlecase(Locale.getDefault()) else it.toString()
                    }
        }
    }

    private fun getTranslation(key: String, lang: String): String {
        val isPl =
                if (lang == "pl") true
                else if (lang == "en") false else Locale.getDefault().language.startsWith("pl")
        return when (key) {
            "outages" -> if (isPl) "wyłączeń" else "outages"
            "setup" -> if (isPl) "Skonfiguruj" else "Setup needed"
            "updating" -> if (isPl) "Aktualizacja..." else "Updating..."
            "inactive" -> if (isPl) "Nieaktywne" else "Inactive"
            else -> key
        }
    }

    abstract suspend fun fetchCount(settings: List<WidgetSettings>): Int

    internal suspend fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
    ) {
        val settingsList = loadSettings(context)
        val language = settingsList?.firstOrNull()?.language ?: "system"
        val theme = settingsList?.firstOrNull()?.theme ?: "system"
        val dark = isDarkMode(context, theme)
        val sourceEnabled = settingsList?.firstOrNull()?.sourceEnabled ?: true

        val activeSettings = settingsList?.filter { it.isActive } ?: emptyList()
        val addressCount = activeSettings.size

        var count = "?"
        var statusMessage: String? = null

        if (!sourceEnabled) {
            count = "–"
            statusMessage = getTranslation("inactive", language)
        } else if (settingsList == null || activeSettings.isEmpty()) {
            count = "?"
            statusMessage = getTranslation("setup", language)
        } else {
            try {
                val total = fetchCount(activeSettings)
                count = if (addressCount > 1) "$total ($addressCount)" else total.toString()
            } catch (e: Exception) {
                count = "!"
                statusMessage = "Error"
            }
        }

        val updatedAt =
                statusMessage
                        ?: run {
                            val timeFormat = SimpleDateFormat("HH:mm", Locale.getDefault())
                            timeFormat.format(Date())
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
                if (count == "0" ||
                                count.startsWith("0 (") ||
                                count == "?" ||
                                count == "!" ||
                                count == "–"
                ) {
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
                        PendingIntent.getActivity(
                                context,
                                0,
                                launchIntent,
                                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                        )
                    } else {
                        refreshPending
                    }
                }

        // Always refresh when clicking the background
        views.setOnClickPendingIntent(R.id.widget_root, refreshPending)

        // If there are outages, clicking the icon or the count will open the app
        views.setOnClickPendingIntent(R.id.widget_icon, clickPending)
        views.setOnClickPendingIntent(R.id.widget_count, clickPending)

        applyTheme(views, dark)
        views.setTextViewText(R.id.widget_label, getTranslation(labelKey, language))
        views.setTextViewText(R.id.widget_count, count)
        views.setTextViewText(R.id.widget_updated, updatedAt)

        return views
    }

    protected fun isWroclaw(settings: WidgetSettings): Boolean {
        val name = settings.cityName.lowercase()
        return name == "wrocław" || name == "wroclaw" || settings.cityId == 969400L
    }

    protected fun isWarszawa(settings: WidgetSettings): Boolean {
        val name = settings.cityName.lowercase()
        return name == "warszawa" || name == "warsaw" || settings.cityId == 918123L
    }

    protected fun isInPgeRegion(settings: WidgetSettings): Boolean {
        val v = settings.voivodeship.lowercase()
        return v.contains("lubelskie") ||
                v.contains("podlaskie") ||
                v.contains("łódzkie") ||
                v.contains("świętokrzyskie") ||
                v.contains("mazowieckie") ||
                v.contains("małopolskie") ||
                v.contains("podkarpackie")
    }

    protected fun isInEnergaRegion(settings: WidgetSettings): Boolean {
        val v = settings.voivodeship.lowercase()
        return v.contains("pomorskie") ||
                v.contains("warmińsko") ||
                v.contains("zachodniopomorskie") ||
                v.contains("wielkopolskie") ||
                v.contains("kujawsko") ||
                v.contains("mazowieckie")
    }

    protected fun fetchTauronAlertCount(settingsList: List<WidgetSettings>): Int {
        var totalCount = 0
        for (settings in settingsList) {
            try {
                // Look up city GAID
                val cityGAID = lookupTauronCity(settings) ?: continue

                // Check for city without streets (TERYT streetId == 0)
                val streetGAID =
                        if (settings.streetName1.isEmpty()) {
                            lookupTauronStreetDummy(cityGAID)
                        } else {
                            val streetQuery = settings.streetName1
                            lookupTauronStreet(streetQuery, cityGAID)
                        }
                                ?: continue

                // Fetch outages
                val dateFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
                dateFormat.timeZone = TimeZone.getTimeZone("UTC")
                val now = dateFormat.format(Date())
                val baseUrl = "https://www.tauron-dystrybucja.pl/waapi/outages/address"
                val safeHouseNo =
                        java.net.URLEncoder.encode(settings.houseNo, "utf-8").replace("+", "%20")
                val params =
                        "cityGAID=$cityGAID&streetGAID=$streetGAID" +
                                "&houseNo=$safeHouseNo" +
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
                    continue
                }

                val response = conn.inputStream.bufferedReader().readText()
                conn.disconnect()

                totalCount += parseOutageItems(response, settings)
            } catch (e: Exception) {
                continue
            }
        }
        return totalCount
    }

    private fun lookupTauronCity(settings: WidgetSettings): Long? {
        return try {
            val encoded = java.net.URLEncoder.encode(settings.cityName, "utf-8").replace("+", "%20")
            val url =
                    URL(
                            "https://www.tauron-dystrybucja.pl/waapi/enum/geo/cities?partName=$encoded&_=${System.currentTimeMillis()}"
                    )
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
                return null
            }

            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            val items = org.json.JSONArray(response)
            for (i in 0 until items.length()) {
                val item = items.getJSONObject(i)
                val prov = item.optString("ProvinceName", "")
                val dist = item.optString("DistrictName", "")
                val comm = item.optString("CommuneName", "")

                if (prov.equals(settings.voivodeship, ignoreCase = true) &&
                                dist.equals(settings.district, ignoreCase = true) &&
                                comm.equals(settings.commune, ignoreCase = true)
                ) {
                    return item.getLong("GAID")
                }
            }
            if (items.length() > 0) items.getJSONObject(0).getLong("GAID") else null
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronStreetDummy(cityGAID: Long): Long? {
        return try {
            val url =
                    URL(
                            "https://www.tauron-dystrybucja.pl/waapi/enum/geo/onlyonestreet?ownerGAID=$cityGAID&_=${System.currentTimeMillis()}"
                    )
            val conn = url.openConnection() as HttpURLConnection
            conn.connectTimeout = 10000
            conn.readTimeout = 10000
            if (conn.responseCode !in 200..299) {
                conn.disconnect()
                return null
            }
            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()
            val items = org.json.JSONArray(response)
            if (items.length() > 0) items.getJSONObject(0).getLong("GAID") else null
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronStreet(streetName: String, cityGAID: Long): Long? {
        return try {
            val encoded = java.net.URLEncoder.encode(streetName, "utf-8").replace("+", "%20")
            val url =
                    URL(
                            "https://www.tauron-dystrybucja.pl/waapi/enum/geo/streets?partName=$encoded&ownerGAID=$cityGAID&_=${System.currentTimeMillis()}"
                    )
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
                return null
            }

            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            val json = JSONObject("{\"items\":$response}")
            val items = json.optJSONArray("items") ?: return null
            if (items.length() == 0) return null
            items.getJSONObject(0).getLong("GAID")
        } catch (e: Exception) {
            null
        }
    }

    protected fun fetchMpwikAlertCount(settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isWroclaw(it) }
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

    protected fun fetchFortumAlertCount(settingsList: List<WidgetSettings>): Int {
        val citiesUrl = URL("https://formularz.fortum.pl/api/v1/teryt/cities")
        Log.i(TAG, "Fortum: GET $citiesUrl")
        val citiesJson =
                try {
                    fetchJson(citiesUrl)
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

                Log.i(TAG, "Fortum API: planned=$plannedUrl, current=$currentUrl")

                val plannedRes = fetchJson(plannedUrl)
                val currentRes = fetchJson(currentUrl)

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

    internal fun parseFortumItems(jsonString: String, settings: WidgetSettings): Int {
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
            if (endDateStr.isNotEmpty()) {
                try {
                    val end = isoFormat.parse(endDateStr)
                    if (end != null && end.before(now)) continue
                } catch (e: Exception) {}
            }

            val message = item.optString("message", "")
            if (matchesStreetOnly(message, settings.streetName1, settings.streetName2)) count++
        }
        return count
    }

    internal fun parseMpwikItems(jsonString: String, settings: WidgetSettings): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("failures") ?: return 0
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
            if (matchesStreetOnly(content, settings.streetName1, settings.streetName2)) count++
        }
        return count
    }

    internal fun parseOutageItems(jsonString: String, settings: WidgetSettings): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("OutageItems") ?: return 0
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
            if (matchesStreetOnly(message, settings.streetName1, settings.streetName2)) count++
        }
        return count
    }

    protected fun fetchEnergaAlertCount(settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isInEnergaRegion(it) }
        if (relevantSettings.isEmpty()) return 0

        var totalCount = 0
        try {
            val apiUrl = fetchEnergaApiUrl() ?: return 0
            val response = fetchJson(URL(apiUrl))
            val json = JSONObject(response)
            val shutdowns =
                    json.optJSONObject("document")
                            ?.optJSONObject("payload")
                            ?.optJSONArray("shutdowns")
                            ?: return 0

            val now = Date()
            val formats =
                    listOf(
                            SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US),
                            SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US)
                    )

            for (settings in settingsList) {
                var count = 0
                for (i in 0 until shutdowns.length()) {
                    val s = shutdowns.getJSONObject(i)
                    val endStr = s.optString("endDate", "")
                    if (endStr.isNotEmpty()) {
                        var end: Date? = null
                        for (fmt in formats) {
                            try {
                                end = fmt.parse(endStr)
                                if (end != null) break
                            } catch (e: Exception) {}
                        }
                        if (end != null && end.before(now)) continue
                    }
                    val message = s.optString("message", "")
                    val areas = s.optJSONArray("areas")

                    val cityMatch = wordMatch(message, settings.cityName)
                    var communeMatch = false
                    if (areas != null) {
                        for (j in 0 until areas.length()) {
                            if (wordMatch(areas.getString(j), settings.commune)) {
                                communeMatch = true
                                break
                            }
                        }
                    }

                    if (cityMatch &&
                                    communeMatch &&
                                    matchesStreetOnly(
                                            message,
                                            settings.streetName1,
                                            settings.streetName2
                                    )
                    )
                            count++
                }
                totalCount += count
            }
        } catch (e: Exception) {
            Log.e(TAG, "Energa sync failed", e)
        }
        return totalCount
    }

    private fun fetchEnergaApiUrl(): String? {
        return try {
            val url =
                    URL(
                            "https://www.energa-operator.pl/uslugi/awarie-i-wylaczenia/wylaczenia-planowane"
                    )
            val html = URL(url.toString()).readText()
            val regex = Regex("""data-shutdowns="([^"]+)"""")
            val match = regex.find(html)
            match?.groupValues?.get(1)?.let { "https://energa-operator.pl$it" }
        } catch (e: Exception) {
            null
        }
    }

    private fun getEneaRegionsForDistrict(district: String): List<Int> {
        val d = district.lowercase().removePrefix("m. ")
        return when (d) {
            "zielonogórski", "zielona góra" -> listOf(1)
            "żarski", "żagański" -> listOf(2)
            "wolsztyński" -> listOf(3)
            "świebodziński" -> listOf(4)
            "nowosolski", "wschowski" -> listOf(5)
            "krośnieński" -> listOf(6)
            "poznański", "poznań", "śremski", "obornicki" -> listOf(7)
            "wałecki" -> listOf(8)
            "wrzesiński", "słupecki", "średzki" -> listOf(9)
            "szamotulski" -> listOf(10)
            "pilski", "piła", "złotowski" -> listOf(11)
            "nowotomyski", "grodziski" -> listOf(12)
            "leszczyński", "leszno", "gostyński", "rawicki" -> listOf(13)
            "kościański" -> listOf(14)
            "gnieźnieński", "gniezno" -> listOf(15)
            "chodzieski", "czarnkowsko-trzcianecki" -> listOf(16)
            "bydgoski", "bydgoszcz" -> listOf(17)
            "świecki", "chełmiński", "tucholski" -> listOf(18)
            "nakielski", "sępoleński" -> listOf(19)
            "mogileński", "żniński" -> listOf(20)
            "inowrocławski", "inowrocław" -> listOf(21)
            "chojnicki", "człuchowski" -> listOf(22)
            "szczeciński", "szczecin", "policki" -> listOf(23)
            "stargardzki", "pyrzycki", "stargard" -> listOf(24)
            "kamieński", "świnoujście" -> listOf(25)
            "gryficki", "łobeski" -> listOf(26)
            "goleniowski" -> listOf(27)
            "gorzowski", "gorzów wlkp.", "gorzów wielkopolski", "strzelecko-drezdenecki" ->
                    listOf(28)
            "sulęciński", "słubicki" -> listOf(29)
            "międzychodzki" -> listOf(30)
            "myśliborski" -> listOf(31)
            "choszczeński" -> listOf(32)
            else -> emptyList()
        }
    }

    protected suspend fun fetchPgeAlertCount(settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isInPgeRegion(it) }
        if (relevantSettings.isEmpty()) return 0

        var totalCount = 0
        try {
            val now = Date()
            val sdf = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US)
            val stopAtFrom = sdf.format(now).replace(" ", "+").replace(":", "%3A")
            val future = Date(now.time + 90L * 24 * 60 * 60 * 1000) // 90 days
            val startAtTo = sdf.format(future).replace(" ", "+").replace(":", "%3A")

            val urlString =
                    "https://power-outage.gkpge.pl/api/power-outage?startAtTo=$startAtTo&stopAtFrom=$stopAtFrom&types[]=2"
            val response = fetchJson(URL(urlString))
            val outages = org.json.JSONArray(response)

            for (settings in relevantSettings) {
                var count = 0
                for (i in 0 until outages.length()) {
                    val outage = outages.getJSONObject(i)
                    val description = outage.optString("description", "")
                    val cityMatchDesc =
                            description.lowercase().contains(settings.cityName.lowercase())
                    val addresses = outage.optJSONArray("addresses") ?: continue
                    var matched = false
                    for (j in 0 until addresses.length()) {
                        val addr = addresses.getJSONObject(j)
                        val teryt = addr.optJSONObject("teryt")
                        if (teryt != null) {
                            val vMatch =
                                    teryt.optString("voivodeshipName").uppercase() ==
                                            settings.voivodeship.uppercase()
                            if (!vMatch) continue

                            val dMatch =
                                    teryt.optString("countyName").lowercase() ==
                                            settings.district.lowercase()
                            if (!dMatch) continue

                            val cMatch =
                                    teryt.optString("communeName").lowercase() ==
                                            settings.commune.lowercase()
                            if (!cMatch) continue

                            val cityMatch =
                                    teryt.optString("cityName").lowercase() ==
                                            settings.cityName.lowercase()
                            if (!cityMatch) continue

                            val streetQuery = settings.streetName1.lowercase()
                            val streetMatch =
                                    if (streetQuery.isEmpty()) true
                                    else
                                            teryt.optString("streetName")
                                                    .lowercase()
                                                    .contains(streetQuery)

                            if (streetMatch) {
                                matched = true
                                break
                            }
                        } else if (cityMatchDesc) {
                            if (settings.streetName1.isEmpty()) {
                                matched = true
                                break
                            }
                            if (description.lowercase().contains(settings.streetName1.lowercase())
                            ) {
                                matched = true
                                break
                            }
                        }
                    }
                    if (matched) count++
                }
                totalCount += count
            }
        } catch (e: Exception) {
            Log.e(TAG, "PGE sync failed", e)
        }
        return totalCount
    }

    protected suspend fun fetchEneaAlertCount(settingsList: List<WidgetSettings>): Int =
            coroutineScope {
                var totalCount = 0
                try {
                    val targetRegions =
                            settingsList
                                    .flatMap { getEneaRegionsForDistrict(it.district) }
                                    .distinct()
                    if (targetRegions.isEmpty()) return@coroutineScope 0

                    val jobs =
                            targetRegions.map { id ->
                                async(Dispatchers.IO) {
                                    try {
                                        val url =
                                                URL(
                                                        "https://www.wylaczenia-eneaoperator.pl/rss/rss_unpl_$id.xml"
                                                )
                                        val conn = url.openConnection() as HttpURLConnection
                                        conn.requestMethod = "GET"
                                        conn.connectTimeout = 10000
                                        conn.readTimeout = 10000
                                        if (conn.responseCode in 200..299) {
                                            conn.inputStream.bufferedReader().readText()
                                        } else null
                                    } catch (e: Exception) {
                                        null
                                    }
                                }
                            }

                    val xmls = jobs.awaitAll().filterNotNull()
                    val descriptionRegex =
                            Regex("<description>(.*?)</description>", RegexOption.DOT_MATCHES_ALL)

                    for (settings in settingsList) {
                        var count = 0
                        for (xml in xmls) {
                            val items = xml.split("<item>").drop(1)
                            for (itemXml in items) {
                                val descMatch = descriptionRegex.find(itemXml)
                                if (descMatch != null) {
                                    var description = descMatch.groupValues[1]
                                    description =
                                            description
                                                    .trim()
                                                    .removePrefix("<![CDATA[")
                                                    .removeSuffix("]]>")
                                                    .trim()
                                    val cityMatch = wordMatch(description, settings.cityName)

                                    val streetMatches =
                                            matchesStreetOnly(
                                                    description,
                                                    settings.streetName1,
                                                    settings.streetName2
                                            )

                                    if (cityMatch && streetMatches) {
                                        count++
                                    }
                                }
                            }
                        }
                        totalCount += count
                    }
                } catch (e: Exception) {
                    Log.e(TAG, "Enea sync failed", e)
                }
                totalCount
            }

    private fun matchesStreet(
            text: String,
            cityName: String,
            streetName1: String,
            streetName2: String?
    ): Boolean {
        if (text.isEmpty()) return false

        // City must match
        if (!wordMatch(text, cityName)) return false

        // Village case: no street name, match by city name only
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

    private fun matchesStreetOnly(
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

    private fun wordMatch(text: String, word: String): Boolean {
        val escapedWord = java.util.regex.Pattern.quote(word)
        val regex = Regex("\\b$escapedWord\\b", RegexOption.IGNORE_CASE)
        return regex.containsMatchIn(text)
    }

    protected suspend fun fetchStoenAlertCount(settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isWarszawa(it) }
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

            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            val outages = org.json.JSONArray(response)
            val now = Date()
            val stoenFormat = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US)

            for (settings in relevantSettings) {
                var count = 0
                for (i in 0 until outages.length()) {
                    val outage = outages.getJSONObject(i)

                    // Filter by date (End of outage must be in the future)
                    val endStr = outage.optString("outageEnd", "")
                    if (endStr.isNotEmpty()) {
                        try {
                            val end = stoenFormat.parse(endStr)
                            if (end != null && end.before(now)) continue
                        } catch (e: Exception) {}
                    }

                    if (settings.streetName1.isEmpty()) {
                        count++
                        continue
                    }

                    val addresses = outage.optJSONArray("addresses") ?: continue
                    var streetMatched = false
                    for (j in 0 until addresses.length()) {
                        val addr = addresses.getJSONObject(j)
                        val street = addr.optString("streetName", "")
                        if (street.isNotEmpty()) {
                            val streetNorm =
                                    street.lowercase()
                                            .replace("ul. ", "")
                                            .replace("al. ", "")
                                            .replace("pl. ", "")
                                            .replace("os. ", "")
                                            .trim()

                            val query = settings.streetName1.lowercase()
                            // If street names match, we count it (Stoen is matched by street only
                            // per user request)
                            if (streetNorm.contains(query) || query.contains(streetNorm)) {
                                streetMatched = true
                                break
                            }
                        }
                    }
                    if (streetMatched) count++
                }
                totalCount += count
            }
        } catch (e: Exception) {
            Log.e(TAG, "STOEN fetch error", e)
        }
        return totalCount
    }
}
