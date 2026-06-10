package xyz.eremef.awaria

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.util.Log
import android.widget.RemoteViews
import java.text.SimpleDateFormat
import java.util.*
import kotlinx.coroutines.*
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import android.content.res.Configuration

class AllWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ALL"
    override val primaryColorRes: Int = R.color.widget_text_primary
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "status"
    override val sourceKey: String = "all_status"

    override fun showLoadingPlaceholder(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
    ) {
        val views = RemoteViews(context.packageName, R.layout.widget_all_outage)
        val refreshIntent =
                Intent(context, this::class.java).apply {
                    action = refreshAction
                    putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
                }
        val pending =
                PendingIntent.getBroadcast(
                        context,
                        appWidgetId,
                        refreshIntent,
                        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                )
        views.setOnClickPendingIntent(R.id.widget_root, pending)
        views.setTextViewText(R.id.count_power, "–")
        views.setTextViewText(R.id.count_heat, "–")
        views.setTextViewText(R.id.count_water, "–")
        views.setTextViewText(R.id.count_gas, "–")
        views.setTextViewText(R.id.widget_updated, context.getString(R.string.msg_updating))
        views.setTextViewText(R.id.widget_address_name, "")
        // views.setTextViewText(R.id.label_power, context.getString(R.string.label_power))
        // views.setTextViewText(R.id.label_heat, context.getString(R.string.label_heat))
        // views.setTextViewText(R.id.label_water, context.getString(R.string.label_water))
        // views.setTextViewText(R.id.label_gas, context.getString(R.string.label_gas))
        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    override fun onUpdate(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetIds: IntArray
    ) {
        WidgetUtils.initVerifier(context)
        scheduleWork(context)
        // Show placeholder immediately — no goAsync needed, WorkManager does the actual fetch.
        for (appWidgetId in appWidgetIds) {
            showLoadingPlaceholder(context, appWidgetManager, appWidgetId)
        }
        val request = OneTimeWorkRequestBuilder<WidgetUpdateWorker>().build()
        WorkManager.getInstance(context).enqueue(request)
    }

    override suspend fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int,
            useCacheOnly: Boolean
    ) {
        val settingsResult = loadSettings(context)
        val allSettings = settingsResult?.first
        val fullJson = settingsResult?.second

        val primaryAddress =
                allSettings?.find { it.isPrimary } ?: allSettings?.firstOrNull { it.isActive }

        val customAddressId = getStoredAddressId(context, appWidgetId)
        val selectedAddress =
                if (customAddressId != null && allSettings != null) {
                    allSettings.find {
                        "${it.cityId}-${it.streetId}-${it.houseNo}" == customAddressId
                    }
                            ?: primaryAddress
                } else {
                    primaryAddress
                }


        val enabledSources = getEnabledSources(fullJson)
        val allEnabledByDefault = fullJson?.has("enabledSources") == false

        var powerCount = "–"
        var heatCount = "–"
        var waterCount = "–"
        var gasCount = "–"
        var totalOutages = 0
        var updatedAt = ""

        val cached = getAllWidgetData(context, appWidgetId)

        if (useCacheOnly && cached != null) {
            powerCount = cached[0]
            heatCount = cached[1]
            waterCount = cached[2]
            gasCount = cached[3]
            updatedAt = cached[4]
            totalOutages = (powerCount.toIntOrNull() ?: 0) + (heatCount.toIntOrNull() ?: 0) + (waterCount.toIntOrNull() ?: 0) + (if (gasCount == "!") 0 else gasCount.toIntOrNull() ?: 0)
        } else {
            if (selectedAddress != null) {
                val settingsList = listOf(selectedAddress)
                val hash = calculateHash(settingsList)
                val activeSettings = settingsList.filter { it.isActive }

                if (!WidgetUtils.isNetworkAvailable(context)) {
                    if (cached != null) {
                        powerCount = cached[0]
                        heatCount = cached[1]
                        waterCount = cached[2]
                        gasCount = cached[3]
                        updatedAt = cached[4]
                        totalOutages = (powerCount.toIntOrNull() ?: 0) + (heatCount.toIntOrNull() ?: 0) + (waterCount.toIntOrNull() ?: 0) + (if (gasCount == "!") 0 else gasCount.toIntOrNull() ?: 0)
                    } else {
                        powerCount = "!"
                        heatCount = "!"
                        waterCount = "!"
                        gasCount = "!"
                        updatedAt = getTranslation(context, "offline")
                    }
                } else {
                    try {
                        coroutineScope {
                            val settingsJson =
                                    WidgetUtils.serializeSettingsForRust(activeSettings, fullJson)
                            val p = async {
                                val sources = listOf("tauron", "stoen", "energa", "enea", "pge")
                                        .filter { allEnabledByDefault || it in enabledSources }
                                sources
                                        .map { source ->
                                            async {
                                                try {
                                                    ProviderCache.getOrFetch(source, hash) {
                                                        WidgetUtils.fetchCountFromRust(
                                                                context,
                                                                source,
                                                                settingsJson
                                                        )
                                                    }
                                                } catch (e: Exception) {
                                                    Log.w(
                                                            "AllWidget",
                                                            "Failed to fetch $source: ${e.message}"
                                                    )
                                                    0
                                                }
                                            }
                                        }
                                        .awaitAll()
                                        .sum()
                            }
                            val h = async {
                                val heatSources = listOf("fortum", "tauron_heat", "veolia_warszawa", "veolia_poznan", "veolia_lodz", "gpec")
                                        .filter { allEnabledByDefault || it in enabledSources }
                                heatSources
                                        .map { source ->
                                            async {
                                                try {
                                                    ProviderCache.getOrFetch(source, hash) {
                                                        WidgetUtils.fetchCountFromRust(
                                                                context,
                                                                source,
                                                                settingsJson
                                                        )
                                                    }
                                                } catch (e: Exception) {
                                                    Log.w(
                                                            "AllWidget",
                                                            "Failed to fetch $source: ${e.message}"
                                                    )
                                                    0
                                                }
                                            }
                                        }
                                        .awaitAll()
                                        .sum()
                            }
                            val w = async {
                                val waterSources = listOf("mpwik_wroclaw", "mpwik_warszawa", "wmk", "aquanet", "katowickie_wodociagi", "zwik_lodz", "pwik_kalisz", "pwik_czestochowa", "wodociagi_plockie", "gdanskie_wodociagi", "puk_rokietnica")
                                        .filter { allEnabledByDefault || it in enabledSources }
                                waterSources
                                    .map { source ->
                                            async {
                                                try {
                                                    ProviderCache.getOrFetch(source, hash) {
                                                        WidgetUtils.fetchCountFromRust(
                                                                context,
                                                                source,
                                                                settingsJson
                                                        )
                                                    }
                                                } catch (e: Exception) {
                                                    Log.w(
                                                            "AllWidget",
                                                            "Failed to fetch $source: ${e.message}"
                                                    )
                                                    0
                                                }
                                            }
                                        }
                                        .awaitAll()
                                        .sum()
                            }
                            val g = async {
                                if (allEnabledByDefault || "psg" in enabledSources) {
                                    try {
                                        ProviderCache.getOrFetch("psg", hash) {
                                            PsgWebViewFetcher.fetchCount(context, activeSettings)
                                        }
                                    } catch (e: Exception) {
                                        Log.w("AllWidget", "Failed to fetch psg: ${e.message}")
                                        0
                                    }
                                } else {
                                    0
                                }
                            }

                            val resP = p.await()
                            val resH = h.await()
                            val resW = w.await()
                            val resG = g.await()

                            powerCount = resP.toString()
                            heatCount = resH.toString()
                            waterCount = resW.toString()
                            gasCount = if (resG >= 0) resG.toString() else "!"
                            totalOutages = resP + resH + resW + (if (resG >= 0) resG else 0)
                        }
                    } catch (e: Exception) {
                        Log.e("AllWidget", "Error fetching counts", e)
                        if (cached != null) {
                            powerCount = cached[0]
                            heatCount = cached[1]
                            waterCount = cached[2]
                            gasCount = cached[3]
                            updatedAt = cached[4]
                            totalOutages = (powerCount.toIntOrNull() ?: 0) + (heatCount.toIntOrNull() ?: 0) + (waterCount.toIntOrNull() ?: 0) + (if (gasCount == "!") 0 else gasCount.toIntOrNull() ?: 0)
                        } else {
                            powerCount = "!"
                            heatCount = "!"
                            waterCount = "!"
                            gasCount = "!"
                        }
                    }
                }
            }
            if (updatedAt.isEmpty()) {
                updatedAt = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date())
            }
        }

        if (!useCacheOnly) {
            saveAllWidgetData(context, appWidgetId, powerCount, heatCount, waterCount, gasCount, updatedAt)
        }
        val addressName =
                if (selectedAddress != null) {
                    if (selectedAddress.name.isNotEmpty()) selectedAddress.name
                    else
                            "${selectedAddress.cityName}, ${selectedAddress.streetName} ${selectedAddress.houseNo}"
                } else {
                    getTranslation(context, "no_address")
                }

        val views = RemoteViews(context.packageName, R.layout.widget_all_outage)

        // Clicks
        val refreshIntent =
                Intent(context, this::class.java).apply {
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
                if (totalOutages > 0) {
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
                } else {
                    refreshPending
                }

        views.setOnClickPendingIntent(R.id.widget_root, refreshPending)
        views.setOnClickPendingIntent(R.id.section_power, clickPending)
        views.setOnClickPendingIntent(R.id.section_heat, clickPending)
        views.setOnClickPendingIntent(R.id.section_water, clickPending)
        views.setOnClickPendingIntent(R.id.section_gas, clickPending)

        // Texts
        views.setTextViewText(R.id.widget_address_name, addressName)
        views.setTextViewText(R.id.widget_updated, updatedAt)
        views.setTextViewText(R.id.count_power, powerCount)
        views.setTextViewText(R.id.count_heat, heatCount)
        views.setTextViewText(R.id.count_water, waterCount)
        views.setTextViewText(R.id.count_gas, gasCount)

        // Labels
        // views.setTextViewText(R.id.label_power, getTranslation(context, "power"))
        // views.setTextViewText(R.id.label_heat, getTranslation(context, "heat"))
        // views.setTextViewText(R.id.label_water, getTranslation(context, "water"))
        // views.setTextViewText(R.id.label_gas, getTranslation(context, "gas"))

        // Check enabled utilities


        val powerEnabled =
                allEnabledByDefault ||
                        listOf("tauron", "stoen", "energa", "enea", "pge").any {
                            it in enabledSources
                        }
        val heatEnabled =
                allEnabledByDefault || listOf("fortum", "tauron_heat", "veolia_warszawa", "veolia_poznan", "veolia_lodz", "gpec").any { it in enabledSources }
        val waterEnabled =
                allEnabledByDefault || listOf("mpwik_wroclaw", "mpwik_warszawa", "wmk", "aquanet", "katowickie_wodociagi", "zwik_lodz", "pwik_kalisz", "pwik_czestochowa", "wodociagi_plockie", "gdanskie_wodociagi", "puk_rokietnica").any { it in enabledSources }
        val gasEnabled = allEnabledByDefault || listOf("psg").any { it in enabledSources }

        // Theme
        applyAllTheme(
                views,
                powerEnabled,
                heatEnabled,
                waterEnabled,
                gasEnabled
        )

        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    private fun applyAllTheme(
            views: RemoteViews,
            powerEnabled: Boolean,
            heatEnabled: Boolean,
            waterEnabled: Boolean,
            gasEnabled: Boolean
    ) {
        // Gray out disabled utilities
        val disabledAlpha = 0.3f
        views.setFloat(R.id.section_power, "setAlpha", if (powerEnabled) 1.0f else disabledAlpha)
        views.setFloat(R.id.section_heat, "setAlpha", if (heatEnabled) 1.0f else disabledAlpha)
        views.setFloat(R.id.section_water, "setAlpha", if (waterEnabled) 1.0f else disabledAlpha)
        views.setFloat(R.id.section_gas, "setAlpha", if (gasEnabled) 1.0f else disabledAlpha)
    }
}
