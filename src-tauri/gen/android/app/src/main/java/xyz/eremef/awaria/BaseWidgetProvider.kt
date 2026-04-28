package xyz.eremef.awaria

import android.util.Log

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
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONObject

data class WidgetSettings(
        val name: String,
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
        val sourceEnabled: Boolean,
        val isPrimary: Boolean = false
)

/** Global cache to share fetch results between different widgets during the same update cycle. */
object ProviderCache {
    private val cache = ConcurrentHashMap<String, Deferred<Int>>()
    private val mutex = Mutex()
    private var lastClearTime = 0L
    private const val CACHE_TTL_MS = 30000L // 30 seconds

    suspend fun getOrFetch(providerId: String, hash: String, fetch: suspend () -> Int): Int {
        val now = System.currentTimeMillis()
        val key = "$providerId:$hash"

        val deferred = mutex.withLock {
            // Clear cache if stale
            if (now - lastClearTime > CACHE_TTL_MS) {
                cache.clear()
                lastClearTime = now
            }

            cache.getOrPut(key) { CoroutineScope(Dispatchers.IO).async { fetch() } }
        }
        return deferred.await()
    }
}

abstract class BaseWidgetProvider : AppWidgetProvider() {

    companion object {
        const val WORK_NAME = "xyz.eremef.awaria.WIDGET_UPDATE_WORK"
        const val TAG = "AwariaWidget"

        private const val LIGHT_PRIMARY = "#D9006C"
        private const val LIGHT_LABEL = "#666666"
        private const val LIGHT_UPDATED = "#999999"

        private const val DARK_PRIMARY = "#FF4DA6"
        private const val DARK_LABEL = "#A0A0A0"
        private const val DARK_UPDATED = "#777777"
        
        private const val PREFS_NAME = "xyz.eremef.awaria.WidgetPrefs"
        private const val PREF_PREFIX_KEY = "address_"

        internal fun saveAddressId(context: Context, appWidgetId: Int, addressId: String) {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit()
            prefs.putString(PREF_PREFIX_KEY + appWidgetId, addressId)
            prefs.commit()
        }

        internal fun deleteAddressId(context: Context, appWidgetId: Int) {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit()
            prefs.remove(PREF_PREFIX_KEY + appWidgetId)
            prefs.commit()
        }

        internal fun getStoredAddressId(context: Context, appWidgetId: Int): String? {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            return prefs.getString(PREF_PREFIX_KEY + appWidgetId, null)
        }
    }

    abstract val refreshAction: String
    protected abstract val primaryColorRes: Int
    abstract val iconResId: Int
    abstract val labelKey: String
    abstract val sourceKey: String

    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        
        val mgr = AppWidgetManager.getInstance(context)
        val allIds = mgr.getAppWidgetIds(ComponentName(context, this::class.java))
        
        if (intent.action == refreshAction) {
            val appWidgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
            if (appWidgetId != AppWidgetManager.INVALID_APPWIDGET_ID) {
                onUpdate(context, mgr, intArrayOf(appWidgetId))
            } else if (allIds.isNotEmpty()) {
                onUpdate(context, mgr, allIds)
            }
        } else if (intent.action == Intent.ACTION_BOOT_COMPLETED ||
                   intent.action == Intent.ACTION_CONFIGURATION_CHANGED) {
            if (allIds.isNotEmpty()) {
                onUpdate(context, mgr, allIds)
            }
        }
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
                // Parallelize widget updates with a timeout to avoid ANR in goAsync()
                withTimeoutOrNull(15000L) {
                    appWidgetIds
                        .map { appWidgetId ->
                            async { updateWidget(context, appWidgetManager, appWidgetId) }
                        }
                        .awaitAll()
                }
            } catch (e: Exception) {
                Log.e(TAG, "Immediate update timed out or failed: ${e.message}")
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

    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        super.onDeleted(context, appWidgetIds)
        for (appWidgetId in appWidgetIds) {
            deleteAddressId(context, appWidgetId)
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

    internal fun findSettingsFile(context: Context): File? {
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

    internal fun loadSettings(context: Context): List<WidgetSettings>? {
        val settingsFile = findSettingsFile(context) ?: return null
        return try {
            val jsonString = settingsFile.readText(Charsets.UTF_8)
            val json = JSONObject(jsonString)
            val addresses = json.optJSONArray("addresses")
            val enabledSources = json.optJSONArray("enabledSources")
            val primaryIndex =
                    if (json.has("primaryAddressIndex")) json.getInt("primaryAddressIndex") else -1

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
                        true
                    }

            if (addresses != null && addresses.length() > 0) {
                (0 until addresses.length()).map { i ->
                    val addr = addresses.getJSONObject(i)
                    WidgetSettings(
                            name = addr.optString("name", ""),
                            cityName = addr.optString("cityName", ""),
                            voivodeship = addr.optString("voivodeship", ""),
                            district = addr.optString("district", ""),
                            commune = addr.optString("commune", ""),
                            streetName = addr.optString("streetName", ""),
                            streetName1 = addr.optString("streetName1", ""),
                            streetName2 =
                                    addr.optString("streetName2", "").let {
                                        if (it.isEmpty() || it == "null") null else it
                                    },
                            houseNo = addr.optString("houseNo", ""),
                            cityId = addr.optLong("cityId", 0),
                            streetId = addr.optLong("streetId", 0),
                            theme = json.optString("theme", "system"),
                            language = json.optString("language", "system"),
                            isActive = addr.optBoolean("isActive", true),
                            sourceEnabled = isSourceEnabled,
                            isPrimary = (i == primaryIndex)
                    )
                }
            } else {
                null
            }
        } catch (e: Exception) {
            null
        }
    }

    protected fun isDarkMode(context: Context, themeSetting: String): Boolean {
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

    private fun applyTheme(context: Context, views: RemoteViews, themeSetting: String, dark: Boolean) {
        // If system theme is selected, the XML handles background and text colors automatically
        // via resource qualifiers (values/ vs values-night/). We only explicitly set them here
        // to support manual theme overrides (forcing dark/light) and to tint brand-specific elements.
        
        if (themeSetting != "system") {
            val bgRes = if (dark) R.drawable.widget_background_dark else R.drawable.widget_background
            if (bgRes != 0) {
                views.setInt(R.id.widget_root, "setBackgroundResource", bgRes)
            }
            
            val label = context.getColor(if (dark) R.color.widget_text_label else R.color.widget_text_label)
            val updated = context.getColor(if (dark) R.color.widget_text_updated else R.color.widget_text_updated)
            
            views.setTextColor(R.id.widget_label, label)
            views.setTextColor(R.id.widget_updated, updated)
        }
        
        // Brand color and icon tinting always need programmatic application since they differ per provider
        val primary = context.getColor(primaryColorRes)
        views.setTextColor(R.id.widget_count, primary)
        views.setInt(R.id.widget_icon, "setColorFilter", primary)

        if (iconResId != 0) {
            views.setImageViewResource(R.id.widget_icon, iconResId)
        }
    }

    private fun getSourceName(context: Context, key: String): String {
        return when (key) {
            "tauron" -> context.getString(R.string.provider_tauron)
            "stoen" -> context.getString(R.string.provider_stoen)
            "enea" -> context.getString(R.string.provider_enea)
            "energa" -> context.getString(R.string.provider_energa)
            "pge" -> context.getString(R.string.provider_pge)
            "fortum" -> context.getString(R.string.provider_fortum)
            "water" -> context.getString(R.string.provider_water)
            "psg" -> context.getString(R.string.provider_psg)
            else ->
                    key.replaceFirstChar {
                        if (it.isLowerCase()) it.titlecase(Locale.getDefault()) else it.toString()
                    }
        }
    }

    protected fun getTranslation(context: Context, key: String): String {
        return when (key) {
            "outages" -> context.getString(R.string.label_outages)
            "status" -> context.getString(R.string.widget_label_tri)
            "setup" -> context.getString(R.string.msg_setup_needed)
            "updating" -> context.getString(R.string.msg_updating)
            "inactive" -> context.getString(R.string.msg_inactive)
            "power" -> context.getString(R.string.label_power)
            "heat" -> context.getString(R.string.label_heat)
            "water" -> context.getString(R.string.label_water)
            "gas" -> context.getString(R.string.label_gas)
            "no_address" -> context.getString(R.string.msg_no_address)
            "error" -> context.getString(R.string.msg_error)
            else -> key
        }
    }


    protected fun calculateHash(settingsList: List<WidgetSettings>): String {
        return settingsList
                .joinToString("|") { "${it.cityId}-${it.streetId}-${it.houseNo}" }
                .hashCode()
                .toString()
    }

    open internal suspend fun updateWidget(
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
        var count = "?"
        var statusMessage: String? = null

        if (!sourceEnabled) {
            count = "–"
            statusMessage = getTranslation(context, "inactive")
        } else if (settingsList == null || activeSettings.isEmpty()) {
            count = "?"
            statusMessage = getTranslation(context, "setup")
        } else {
            try {
                // Shared fetch result between widgets
                val hash = calculateHash(activeSettings)
                val total =
                        ProviderCache.getOrFetch(sourceKey, hash) {
                            if (sourceKey == "psg") {
                                PsgWebViewFetcher.fetchCount(context, activeSettings)
                            } else {
                                val settingsJson = WidgetUtils.serializeSettingsForRust(activeSettings)
                                WidgetUtils.fetchCountFromRust(context, sourceKey, settingsJson)
                            }
                        }
                count = if (total >= 0) total.toString() else "!"
            } catch (e: Exception) {
                Log.e(TAG, "Error fetching count from Rust for $sourceKey: ${e.message}", e)
                count = "!"
                statusMessage = getTranslation(context, "error")
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
                                            appWidgetId,
                                            R.layout.widget_outage_small,
                                            count,
                                            updatedAt,
                                            language,
                                            theme,
                                            dark
                                    ),
                            SizeF(100f, 100f) to
                                    createRemoteViews(
                                            context,
                                            appWidgetId,
                                            R.layout.widget_outage,
                                            count,
                                            updatedAt,
                                            language,
                                            theme,
                                            dark
                                    ),
                            SizeF(200f, 200f) to
                                    createRemoteViews(
                                            context,
                                            appWidgetId,
                                            R.layout.widget_outage_large,
                                            count,
                                            updatedAt,
                                            language,
                                            theme,
                                            dark
                                    )
                    )
            val views = RemoteViews(viewMapping)
            appWidgetManager.updateAppWidget(appWidgetId, views)
        } else {
            val options = appWidgetManager.getAppWidgetOptions(appWidgetId)
            val minWidth = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
            val minHeight = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT)
            val layoutId =
                    if (minWidth < 100 || minHeight < 100) R.layout.widget_outage_small
                    else if (minWidth < 200 || minHeight < 200) R.layout.widget_outage
                    else R.layout.widget_outage_large
            val views = createRemoteViews(context, appWidgetId, layoutId, count, updatedAt, language, theme, dark)
            appWidgetManager.updateAppWidget(appWidgetId, views)
        }
    }

    private fun createRemoteViews(
            context: Context,
            appWidgetId: Int,
            layoutId: Int,
            count: String,
            updatedAt: String,
            language: String,
            themeSetting: String,
            dark: Boolean
    ): RemoteViews {
        val views = RemoteViews(context.packageName, layoutId)
        val refreshIntent = Intent(context, this::class.java).apply { 
            action = refreshAction 
            putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
        }
        val refreshPending =
                PendingIntent.getBroadcast(
                        context,
                        appWidgetId,
                        refreshIntent,
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                )
        val clickPending =
                if (count == "0" || count == "?" || count == "!" || count == "–") refreshPending
                else {
                    val launchIntent =
                            context.packageManager.getLaunchIntentForPackage(context.packageName)
                                    ?.apply {
                                        flags =
                                                Intent.FLAG_ACTIVITY_NEW_TASK or
                                                        Intent.FLAG_ACTIVITY_CLEAR_TOP
                                    }
                    if (launchIntent != null)
                            PendingIntent.getActivity(
                                    context,
                                    appWidgetId,
                                    launchIntent,
                                    PendingIntent.FLAG_UPDATE_CURRENT or
                                            PendingIntent.FLAG_IMMUTABLE
                            )
                    else refreshPending
                }
        views.setOnClickPendingIntent(R.id.widget_root, refreshPending)
        views.setOnClickPendingIntent(R.id.widget_icon, clickPending)
        views.setOnClickPendingIntent(R.id.widget_count, clickPending)
        applyTheme(context, views, themeSetting, dark)
        views.setTextViewText(R.id.widget_source, getSourceName(context, sourceKey))
        views.setTextViewText(R.id.widget_label, getTranslation(context, labelKey))
        views.setTextViewText(R.id.widget_count, count)
        views.setTextViewText(R.id.widget_updated, updatedAt)
        return views
    }
}
