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
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
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

    abstract suspend fun fetchCount(context:Context, settings: List<WidgetSettings>): Int

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
                val total = fetchCount(context, activeSettings)
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
}
