package xyz.eremef.awaria

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import android.util.SizeF
import android.widget.RemoteViews
import androidx.core.content.edit
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
import kotlin.time.Duration.Companion.milliseconds

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
        val isPrimary: Boolean = false,
        val filterByHouseNo: Boolean = false
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

        val deferred =
                mutex.withLock {
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

        private const val PREFS_NAME = "xyz.eremef.awaria.WidgetPrefs"
        private const val PREF_PREFIX_KEY = "address_"
        private const val PREF_COUNT_PREFIX = "count_"
        private const val PREF_UPDATED_PREFIX = "updated_"
        private const val PREF_CONFIGURED_PREFIX = "configured_"

        internal fun hasWidgetConfiguration(context: Context, appWidgetId: Int): Boolean {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            return prefs.getBoolean(PREF_CONFIGURED_PREFIX + appWidgetId, false) ||
                    prefs.contains("tri_updated_$appWidgetId") ||
                    prefs.contains("all_updated_$appWidgetId")
        }

        internal fun markWidgetConfigured(context: Context, appWidgetId: Int) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit {
                putBoolean(PREF_CONFIGURED_PREFIX + appWidgetId, true)
            }
        }

        internal fun saveAddressId(context: Context, appWidgetId: Int, addressId: String) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit {
                putString(PREF_PREFIX_KEY + appWidgetId, addressId)
            }
        }

        internal fun deleteAddressId(context: Context, appWidgetId: Int) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit {
                remove(PREF_PREFIX_KEY + appWidgetId)
            }
        }

        internal fun getStoredAddressId(context: Context, appWidgetId: Int): String? {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            return prefs.getString(PREF_PREFIX_KEY + appWidgetId, null)
        }

        internal fun getEnabledSources(json: JSONObject?): Set<String> {
            val enabledSources = json?.optJSONArray("enabledSources") ?: return emptySet()
            val set = mutableSetOf<String>()
            for (i in 0 until enabledSources.length()) {
                set.add(enabledSources.getString(i))
            }
            return set
        }

        internal fun saveWidgetData(context: Context, appWidgetId: Int, count: String, updatedAt: String) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit {
                putString(PREF_COUNT_PREFIX + appWidgetId, count)
                putString(PREF_UPDATED_PREFIX + appWidgetId, updatedAt)
            }
        }

        internal fun getWidgetData(context: Context, appWidgetId: Int): Pair<String, String>? {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            val count = prefs.getString(PREF_COUNT_PREFIX + appWidgetId, null)
            val updatedAt = prefs.getString(PREF_UPDATED_PREFIX + appWidgetId, null)
            if (count != null && updatedAt != null) {
                return Pair(count, updatedAt)
            }
            return null
        }

        internal fun saveTriWidgetData(context: Context, appWidgetId: Int, power: String, heat: String, water: String, updatedAt: String) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit {
                putString("tri_power_$appWidgetId", power)
                putString("tri_heat_$appWidgetId", heat)
                putString("tri_water_$appWidgetId", water)
                putString("tri_updated_$appWidgetId", updatedAt)
            }
        }

        internal fun getTriWidgetData(context: Context, appWidgetId: Int): List<String>? {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            val power = prefs.getString("tri_power_$appWidgetId", null) ?: return null
            val heat = prefs.getString("tri_heat_$appWidgetId", null) ?: return null
            val water = prefs.getString("tri_water_$appWidgetId", null) ?: return null
            val updated = prefs.getString("tri_updated_$appWidgetId", null) ?: return null
            return listOf(power, heat, water, updated)
        }

        internal fun saveAllWidgetData(context: Context, appWidgetId: Int, power: String, heat: String, water: String, gas: String, updatedAt: String) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE).edit {
                putString("all_power_$appWidgetId", power)
                putString("all_heat_$appWidgetId", heat)
                putString("all_water_$appWidgetId", water)
                putString("all_gas_$appWidgetId", gas)
                putString("all_updated_$appWidgetId", updatedAt)
            }
        }

        internal fun getAllWidgetData(context: Context, appWidgetId: Int): List<String>? {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            val power = prefs.getString("all_power_$appWidgetId", null) ?: return null
            val heat = prefs.getString("all_heat_$appWidgetId", null) ?: return null
            val water = prefs.getString("all_water_$appWidgetId", null) ?: return null
            val gas = prefs.getString("all_gas_$appWidgetId", null) ?: return null
            val updated = prefs.getString("all_updated_$appWidgetId", null) ?: return null
            return listOf(power, heat, water, gas, updated)
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
            val appWidgetId =
                    intent.getIntExtra(
                            AppWidgetManager.EXTRA_APPWIDGET_ID,
                            AppWidgetManager.INVALID_APPWIDGET_ID
                    )
            if (appWidgetId != AppWidgetManager.INVALID_APPWIDGET_ID) {
                onUpdate(context, mgr, intArrayOf(appWidgetId))
            } else if (allIds.isNotEmpty()) {
                onUpdate(context, mgr, allIds)
            }
        } else if (intent.action == Intent.ACTION_BOOT_COMPLETED ||
                intent.action == Intent.ACTION_CONFIGURATION_CHANGED
        ) {
            if (allIds.isNotEmpty()) {
                onUpdateWithCache(context, mgr, allIds)
            }
        }
    }

    private fun onUpdateWithCache(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetIds: IntArray
    ) {
        WidgetUtils.initVerifier(context)
        val pendingResult = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                delay(300.milliseconds) // Wait for system configuration to settle
                appWidgetIds
                        .map { appWidgetId ->
                            async { updateWidget(context, appWidgetManager, appWidgetId, true) }
                        }
                        .awaitAll()
            } catch (e: Exception) {
                Log.e(TAG, "Cache update failed: ${e.message}")
            } finally {
                pendingResult.finish()
            }
        }
    }

    override fun onUpdate(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetIds: IntArray
    ) {
        WidgetUtils.initVerifier(context)
        scheduleWork(context)
        val pendingResult = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                delay(300.milliseconds) // Wait for system theme configuration to settle
                withTimeoutOrNull(9000L.milliseconds) {
                    appWidgetIds
                            .map { appWidgetId ->
                                async { updateWidget(context, appWidgetManager, appWidgetId) }
                            }
                            .awaitAll()
                } ?: Log.w(TAG, "Update timed out for ${appWidgetIds.contentToString()}")
            } catch (e: Exception) {
                Log.e(TAG, "Update failed: ${e.message}")
            } finally {
                pendingResult.finish()
            }
        }
    }

    open fun showLoadingPlaceholder(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
    ) {
        // Subclasses override this for their layout.
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
                // Resizing only changes presentation. Re-render from the last known value and
                // leave a neutral state when there is no cache; a resize should not start I/O.
                if ((sourceKey == "tri_status" && getTriWidgetData(context, appWidgetId) == null) ||
                        (sourceKey == "all_status" && getAllWidgetData(context, appWidgetId) == null)) {
                    showLoadingPlaceholder(context, appWidgetManager, appWidgetId)
                } else {
                    updateWidget(context, appWidgetManager, appWidgetId, useCacheOnly = true)
                }
            } finally {
                pendingResult.finish()
            }
        }
    }

    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        super.onDeleted(context, appWidgetIds)
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        for (appWidgetId in appWidgetIds) {
            deleteAddressId(context, appWidgetId)
            prefs.edit {
                remove(PREF_CONFIGURED_PREFIX + appWidgetId)
                remove(PREF_COUNT_PREFIX + appWidgetId)
                remove(PREF_UPDATED_PREFIX + appWidgetId)
                remove("tri_power_$appWidgetId")
                remove("tri_heat_$appWidgetId")
                remove("tri_water_$appWidgetId")
                remove("tri_updated_$appWidgetId")
                remove("all_power_$appWidgetId")
                remove("all_heat_$appWidgetId")
                remove("all_water_$appWidgetId")
                remove("all_gas_$appWidgetId")
                remove("all_updated_$appWidgetId")
            }
        }
    }

    override fun onRestored(context: Context, oldWidgetIds: IntArray, newWidgetIds: IntArray) {
        super.onRestored(context, oldWidgetIds, newWidgetIds)
        if (oldWidgetIds.isEmpty() || oldWidgetIds.size != newWidgetIds.size) return
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val fields = listOf(
                PREF_PREFIX_KEY,
                PREF_COUNT_PREFIX,
                PREF_UPDATED_PREFIX,
                PREF_CONFIGURED_PREFIX,
                "tri_power_", "tri_heat_", "tri_water_", "tri_updated_",
                "all_power_", "all_heat_", "all_water_", "all_gas_", "all_updated_"
        )
        val oldIds = oldWidgetIds.toSet()
        val pairs = oldWidgetIds.indices.map { oldWidgetIds[it] to newWidgetIds[it] }
                .filter { (oldId, newId) -> oldId != newId }
        if (pairs.any { (_, newId) ->
                    newId !in oldIds && fields.any { prefs.contains(it + newId) }
                }) return
        val snapshots = pairs.map { (oldId, newId) ->
            Triple(oldId, newId, fields.mapNotNull { prefix ->
                val key = prefix + oldId
                if (prefs.contains(key)) key to prefs.all[key] else null
            })
        }
        prefs.edit {
            snapshots.forEach { (oldId, _, values) -> values.forEach { (key, _) -> remove(key) } }
            snapshots.forEach { (_, newId, values) ->
                values.forEach { (key, value) ->
                    val newKey = key.substringBeforeLast('_') + "_" + newId
                    when (value) {
                        is String -> putString(newKey, value)
                        is Boolean -> putBoolean(newKey, value)
                        is Int -> putInt(newKey, value)
                        is Long -> putLong(newKey, value)
                        is Float -> putFloat(newKey, value)
                        is Set<*> -> putStringSet(newKey, value.filterIsInstance<String>().toSet())
                    }
                }
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

    internal fun scheduleWork(context: Context) {
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

    internal fun loadSettings(
            context: Context,
            sourceId: String = sourceKey
    ): Pair<List<WidgetSettings>, JSONObject?>? {
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
                            if (enabledSources.getString(i) == sourceId) {
                                found = true
                                break
                            }
                        }
                        found
                    } else {
                        true
                    }

            if (addresses != null && addresses.length() > 0) {
                val list =
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
                                    isPrimary = (i == primaryIndex),
                                    filterByHouseNo = json.optBoolean("filterByHouseNo", false)
                            )
                        }
                Pair(list, json)
            } else {
                null
            }
        } catch (_: Exception) {
            null
        }
    }

    protected fun getSourceName(context: Context, key: String): String {
        SingleProviderCatalog.find(key)?.let { return it.displayName(context) }
        return when (key) {
            "tauron" -> context.getString(R.string.provider_tauron)
            "tauron_heat" -> context.getString(R.string.provider_tauron_heat)
            "stoen" -> context.getString(R.string.provider_stoen)
            "enea" -> context.getString(R.string.provider_enea)
            "eco_sa" -> context.getString(R.string.provider_eco_sa)
            "energa" -> context.getString(R.string.provider_energa)
            "pge" -> context.getString(R.string.provider_pge)
            "pge_torun_heat" -> context.getString(R.string.provider_pge_torun_heat)
            "epec_elblag" -> context.getString(R.string.provider_epec_elblag)
            "pec_gliwice" -> context.getString(R.string.provider_pec_gliwice)
            "fortum" -> context.getString(R.string.provider_fortum)
            "mpwik_wroclaw" -> context.getString(R.string.provider_mpwik_wroclaw)
            "mpwik_warszawa" -> context.getString(R.string.provider_mpwik_warszawa)
            "psg" -> context.getString(R.string.provider_psg)
            "wmk" -> context.getString(R.string.provider_wmk)
            "wm_radom" -> context.getString(R.string.provider_wm_radom)
            "aquanet" -> context.getString(R.string.provider_aquanet)
            "aqua_bielsko" -> context.getString(R.string.provider_aqua_bielsko)
            "katowickie_wodociagi" -> context.getString(R.string.provider_katowickie_wodociagi)
            "veolia_warszawa" -> context.getString(R.string.provider_veolia_warszawa)
            "veolia_poznan" -> context.getString(R.string.provider_veolia_poznan)
            "veolia_lodz" -> context.getString(R.string.provider_veolia_lodz)
            "zwik_lodz" -> context.getString(R.string.provider_zwik_lodz)
            "zwik_zielona_gora" -> context.getString(R.string.provider_zwik_zielona_gora)
            "pwik_kalisz" -> context.getString(R.string.provider_pwik_kalisz)
            "wodociagi_plockie" -> context.getString(R.string.provider_wodociagi_plockie)
            "wodociagi_bialostockie" -> context.getString(R.string.provider_wodociagi_bialostockie)
            "wodociagi_kieleckie" -> context.getString(R.string.provider_wodociagi_kieleckie)
            "torunskie_wodociagi" -> context.getString(R.string.provider_torunskie_wodociagi)
            "pwik_czestochowa" -> context.getString(R.string.provider_pwik_czestochowa)
            "pwik_olsztyn" -> context.getString(R.string.provider_pwik_olsztyn)
            "pwik_gorzow" -> context.getString(R.string.provider_pwik_gorzow)
            "rpwik_tychy" -> context.getString(R.string.provider_rpwik_tychy)
            "pwik_gliwice" -> context.getString(R.string.provider_pwik_gliwice)
            "wpwik_walbrzych" -> context.getString(R.string.provider_wpwik_walbrzych)
            "mwik_koszalin" -> context.getString(R.string.provider_mwik_koszalin)
            "sosnowieckie_wodociagi" -> context.getString(R.string.provider_sosnowieckie_wodociagi)
            "tarnowskie_wodociagi" -> context.getString(R.string.provider_tarnowskie_wodociagi)
            "zabrzanskie_wodociagi" -> context.getString(R.string.provider_zabrzanskie_wodociagi)
            "pwik_ruda_slaska" -> context.getString(R.string.provider_pwik_ruda_slaska)
            "pwik_rybnik" -> context.getString(R.string.provider_pwik_rybnik)
            "wik_opole" -> context.getString(R.string.provider_wik_opole)
            "bytomskie_wodociagi" -> context.getString(R.string.provider_bytomskie_wodociagi)
            "dabrowskie_wodociagi" -> context.getString(R.string.provider_dabrowskie_wodociagi)
            "gdanskie_wodociagi" -> context.getString(R.string.provider_gdanskie_wodociagi)
            "gpec" -> context.getString(R.string.provider_gpec)
            "opec_gdynia" -> context.getString(R.string.provider_opec_gdynia)
            "kpec_bydgoszcz" -> context.getString(R.string.provider_kpec_bydgoszcz)
            "enea_cieplo" -> context.getString(R.string.provider_enea_cieplo)
            "energa_cieplo_kaliskie" -> context.getString(R.string.provider_energa_cieplo_kaliskie)
            "puk_rokietnica" -> context.getString(R.string.provider_puk_rokietnica)
            "sec" -> context.getString(R.string.provider_sec)
            "lpec" -> context.getString(R.string.provider_lpec)
            "mpec_kielce" -> context.getString(R.string.provider_mpec_kielce)
            "mpec_rzeszow" -> context.getString(R.string.provider_mpec_rzeszow)
            "mpwik_lublin" -> context.getString(R.string.provider_mpwik_lublin)
            "mpwik_rzeszow" -> context.getString(R.string.provider_mpwik_rzeszow)
            "mwik_bydgoszcz" -> context.getString(R.string.provider_mwik_bydgoszcz)
            "pewik_gdynia" -> context.getString(R.string.provider_pewik_gdynia)
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
            "offline" -> context.getString(R.string.msg_offline)
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
            appWidgetId: Int,
            useCacheOnly: Boolean = false
    ) {
        val settingsResult = loadSettings(context)
        val settingsList = settingsResult?.first
        val fullJson = settingsResult?.second

        val sourceEnabled = settingsList?.firstOrNull()?.sourceEnabled ?: true

        val activeSettings = settingsList?.filter { it.isActive } ?: emptyList()
        var count: String
        var statusMessage: String? = null

        val cachedData = getWidgetData(context, appWidgetId)

        if (useCacheOnly) {
            if (cachedData != null) {
                count = cachedData.first
                statusMessage = if (count == "–") getTranslation(context, "inactive")
                                else if (count == "?") getTranslation(context, "setup")
                                else if (count == "!") getTranslation(context, "offline")
                                else null
            } else {
                count = "–"
                statusMessage = context.getString(R.string.widget_no_cached_data)
            }
        } else {
            if (!sourceEnabled) {
                count = "–"
                statusMessage = getTranslation(context, "inactive")
            } else if (settingsList == null || activeSettings.isEmpty()) {
                count = "?"
                statusMessage = getTranslation(context, "setup")
            } else if (!WidgetUtils.isNetworkAvailable(context)) {
                if (cachedData != null) {
                    count = cachedData.first
                } else {
                    count = "!"
                    statusMessage = getTranslation(context, "offline")
                }
            } else {
                try {
                    val hash = calculateHash(activeSettings)
                    val total =
                            ProviderCache.getOrFetch(sourceKey, hash) {
                                if (sourceKey == "psg") {
                                    PsgWebViewFetcher.fetchCount(context, activeSettings)
                                } else {
                                    val settingsJson =
                                            WidgetUtils.serializeSettingsForRust(
                                                    activeSettings,
                                                    fullJson
                                            )
                                    WidgetUtils.fetchCountFromRust(context, sourceKey, settingsJson)
                                }
                            }
                    count = if (total >= 0) total.toString() else "!"
                } catch (e: Exception) {
                    Log.e(TAG, "Error fetching count from Rust for $sourceKey: ${e.message}", e)
                    if (cachedData != null) {
                        count = cachedData.first
                    } else {
                        count = "!"
                        statusMessage = getTranslation(context, "error")
                    }
                }
            }
        }

        val updatedAt = if (useCacheOnly && cachedData != null) {
            cachedData.second
        } else {
            statusMessage
                    ?: run {
                        val timeFormat = SimpleDateFormat("HH:mm", Locale.getDefault())
                        timeFormat.format(Date())
                    }
        }

        if (!useCacheOnly) {
            saveWidgetData(context, appWidgetId, count, updatedAt)
        }

        val catalogEntry = SingleProviderCatalog.find(sourceKey)
        if (catalogEntry != null) {
            renderCatalogWidget(
                context,
                appWidgetManager,
                appWidgetId,
                catalogEntry,
                count,
                updatedAt,
                refreshAction
            )
            return
        }

        val layoutId = when (sourceKey) {
            "tauron", "stoen", "enea", "energa", "pge" -> R.layout.widget_outage_power
            "enea_cieplo", "eco_sa", "energa_cieplo_kaliskie", "fortum", "tauron_heat", "veolia_warszawa", "veolia_poznan", "veolia_lodz", "gpec", "kpec_bydgoszcz", "mpec_kielce", "opec_gdynia", "sec", "lpec", "pge_torun_heat", "epec_elblag", "pec_gliwice" -> R.layout.widget_outage_heat
            "psg" -> R.layout.widget_outage_gas
            else -> R.layout.widget_outage_water
        }
        
        val layoutSmallId = when (sourceKey) {
            "tauron", "stoen", "enea", "energa", "pge" -> R.layout.widget_outage_power_small
            "enea_cieplo", "eco_sa", "energa_cieplo_kaliskie", "fortum", "tauron_heat", "veolia_warszawa", "veolia_poznan", "veolia_lodz", "gpec", "kpec_bydgoszcz", "mpec_kielce", "opec_gdynia", "sec", "lpec", "pge_torun_heat", "epec_elblag", "pec_gliwice" -> R.layout.widget_outage_heat_small
            "psg" -> R.layout.widget_outage_gas_small
            else -> R.layout.widget_outage_water_small
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val viewMapping =
                    mapOf(
                            SizeF(40f, 40f) to
                                    createRemoteViews(
                                            context,
                                            appWidgetId,
                                            layoutSmallId,
                                            count,
                                            updatedAt
                                    ),
                            SizeF(100f, 100f) to
                                    createRemoteViews(
                                            context,
                                            appWidgetId,
                                            layoutId,
                                            count,
                                            updatedAt
                                    )
                    )
            val views = RemoteViews(viewMapping)
            appWidgetManager.updateAppWidget(appWidgetId, views)
        } else {
            val options = appWidgetManager.getAppWidgetOptions(appWidgetId)
            val minWidth = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
            val minHeight = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT)
            val chosenLayoutId =
                    if (minWidth < 100 || minHeight < 100) layoutSmallId
                    else layoutId
            val views =
                    createRemoteViews(
                            context,
                            appWidgetId,
                            chosenLayoutId,
                            count,
                            updatedAt
                    )
            appWidgetManager.updateAppWidget(appWidgetId, views)
        }
    }
    protected fun createRemoteViews(
            context: Context,
            appWidgetId: Int,
            layoutId: Int,
            count: String,
            updatedAt: String,
            sourceId: String = sourceKey,
            action: String = refreshAction,
            includeIcon: Boolean? = null,
            iconResource: Int? = null,
            iconTintResource: Int? = null,
            countTintResource: Int? = null
    ): RemoteViews {
        val views = RemoteViews(context.packageName, layoutId)
        val refreshIntent =
                Intent(context, this::class.java).apply {
                    this.action = action
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
        views.setOnClickPendingIntent(R.id.widget_count, clickPending)
        
        val isSmall = layoutId == R.layout.widget_outage_power_small || 
                      layoutId == R.layout.widget_outage_heat_small || 
                      layoutId == R.layout.widget_outage_water_small || 
                      layoutId == R.layout.widget_outage_gas_small

        val hasIcon = includeIcon ?: !isSmall
        if (hasIcon) {
            views.setOnClickPendingIntent(R.id.widget_icon, clickPending)
        }

        views.setTextViewText(R.id.widget_source, getSourceName(context, sourceId))
        views.setTextViewText(R.id.widget_count, count)
        views.setTextViewText(R.id.widget_updated, updatedAt)
        countTintResource?.let { views.setTextColor(R.id.widget_count, context.getColor(it)) }

        if (hasIcon) {
            iconResource?.let { views.setImageViewResource(R.id.widget_icon, it) }
            iconTintResource?.let {
                views.setInt(R.id.widget_icon, "setColorFilter", context.getColor(it))
            }
            val iconDesc = when (sourceId) {
                "tauron", "stoen", "enea", "energa", "pge" -> getTranslation(context, "power")
                "enea_cieplo", "eco_sa", "energa_cieplo_kaliskie", "fortum", "tauron_heat", "veolia_warszawa", "veolia_poznan", "veolia_lodz", "gpec", "kpec_bydgoszcz", "sec", "lpec", "mpec_kielce", "mpec_rzeszow", "opec_gdynia", "pge_torun_heat", "epec_elblag" -> getTranslation(context, "heat")
                "mpwik_wroclaw", "mpwik_warszawa", "wmk", "wm_radom", "aquanet", "aqua_bielsko", "katowickie_wodociagi", "zwik_lodz", "zwik_zielona_gora", "pwik_kalisz", "wodociagi_plockie", "wodociagi_bialostockie", "wodociagi_kieleckie", "torunskie_wodociagi", "pwik_czestochowa", "pwik_olsztyn", "pwik_gorzow", "rpwik_tychy", "pwik_gliwice", "wik_opole", "gdanskie_wodociagi", "puk_rokietnica", "mpwik_lublin", "mpwik_rzeszow", "mwik_bydgoszcz", "mwik_koszalin", "pewik_gdynia", "bytomskie_wodociagi", "dabrowskie_wodociagi", "sosnowieckie_wodociagi", "tarnowskie_wodociagi", "zabrzanskie_wodociagi", "pwik_ruda_slaska", "pwik_rybnik", "wpwik_walbrzych" -> getTranslation(context, "water")
                "psg" -> getTranslation(context, "gas")
                else -> getSourceName(context, sourceKey)
            }
            views.setContentDescription(R.id.widget_icon, iconDesc)
        }

        return views
    }

    /** Render the shared responsive single-provider layouts for an arbitrary catalog entry. */
    protected fun renderCatalogWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int,
            entry: SingleProviderEntry,
            count: String,
            updatedAt: String,
            action: String,
            configurePending: PendingIntent? = null
    ) {
        val compactLayout = R.layout.widget_single_compact
        val wideLayout = R.layout.widget_single_wide
        val tallLayout = R.layout.widget_single_tall
        val expandedLayout = R.layout.widget_single_expanded
        val iconTint = when (entry.utility) {
            WidgetUtility.ELECTRICITY -> R.color.utility_power
            WidgetUtility.HEAT -> R.color.utility_heat
            WidgetUtility.WATER -> R.color.utility_water
            WidgetUtility.GAS -> R.color.utility_gas
        }
        fun sizedViews(layoutId: Int, hasIcon: Boolean) = createRemoteViews(
            context,
            appWidgetId,
            layoutId,
            count,
            updatedAt,
            entry.sourceId,
            action,
            includeIcon = hasIcon,
            iconResource = if (hasIcon) entry.utility.iconRes else null,
            iconTintResource = if (hasIcon) iconTint else null,
            countTintResource = if (count == "0") R.color.widget_text_label else iconTint
        ).also { views ->
            if (hasIcon) {
                views.setContentDescription(R.id.widget_icon, context.getString(entry.utility.labelRes))
            }
            configurePending?.let {
                views.setOnClickPendingIntent(R.id.widget_root, it)
                views.setOnClickPendingIntent(R.id.widget_count, it)
                if (hasIcon) views.setOnClickPendingIntent(R.id.widget_icon, it)
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // The map keys are the smallest supported size for each shape. Android selects
            // the largest matching layout for the host's current widget dimensions.
            val viewMapping = mapOf(
                SizeF(40f, 40f) to sizedViews(compactLayout, hasIcon = false),
                SizeF(100f, 40f) to sizedViews(wideLayout, hasIcon = true),
                SizeF(40f, 100f) to sizedViews(tallLayout, hasIcon = true),
                SizeF(100f, 100f) to sizedViews(expandedLayout, hasIcon = true)
            )
            appWidgetManager.updateAppWidget(appWidgetId, RemoteViews(viewMapping))
        } else {
            val options = appWidgetManager.getAppWidgetOptions(appWidgetId)
            val minWidth = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
            val minHeight = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT)
            val (chosenLayout, hasIcon) = when {
                minWidth >= 100 && minHeight >= 100 -> expandedLayout to true
                minWidth >= 100 -> wideLayout to true
                minHeight >= 100 -> tallLayout to true
                else -> compactLayout to false
            }
            val views = sizedViews(chosenLayout, hasIcon)
            appWidgetManager.updateAppWidget(appWidgetId, views)
        }
    }
}
