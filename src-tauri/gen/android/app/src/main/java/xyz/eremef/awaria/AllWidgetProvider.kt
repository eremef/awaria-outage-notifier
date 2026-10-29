package xyz.eremef.awaria

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import android.util.SizeF
import android.view.View
import android.widget.RemoteViews
import java.text.SimpleDateFormat
import java.util.*
import kotlinx.coroutines.*
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager

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
        val updating = context.getString(R.string.msg_updating)
        val pendingAddress = context.getString(R.string.widget_accessibility_address_pending)
        updateResponsiveWidget(context, appWidgetManager, appWidgetId) { views, _ ->
            views.setOnClickPendingIntent(R.id.widget_root, pending)
            views.setContentDescription(
                    R.id.widget_root,
                    context.getString(R.string.widget_accessibility_refresh)
            )
            listOf(R.id.section_power, R.id.section_heat, R.id.section_water, R.id.section_gas).forEach {
                views.setOnClickPendingIntent(it, pending)
            }
            listOf(R.id.count_power, R.id.count_heat, R.id.count_water, R.id.count_gas).forEach {
                views.setOnClickPendingIntent(it, pending)
            }
            views.setTextViewText(R.id.count_power, "–")
            views.setTextViewText(R.id.count_heat, "–")
            views.setTextViewText(R.id.count_water, "–")
            views.setTextViewText(R.id.count_gas, "–")
            views.setContentDescription(
                    R.id.count_power,
                    context.getString(R.string.widget_accessibility_count_loading, context.getString(R.string.label_power), pendingAddress)
            )
            views.setContentDescription(
                    R.id.count_heat,
                    context.getString(R.string.widget_accessibility_count_loading, context.getString(R.string.label_heat), pendingAddress)
            )
            views.setContentDescription(
                    R.id.count_water,
                    context.getString(R.string.widget_accessibility_count_loading, context.getString(R.string.label_water), pendingAddress)
            )
            views.setContentDescription(
                    R.id.count_gas,
                    context.getString(R.string.widget_accessibility_count_loading, context.getString(R.string.label_gas), pendingAddress)
            )
            views.setTextViewText(R.id.widget_updated, updating)
            views.setContentDescription(
                    R.id.widget_updated,
                    context.getString(R.string.widget_accessibility_updated, updating)
            )
            views.setViewVisibility(R.id.widget_updated, View.VISIBLE)
            views.setTextViewText(R.id.widget_address_name, "")
        }
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
                                val sources = GeneratedProviderCatalog.aggregate("power").map { it.id }
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
                                val heatSources = GeneratedProviderCatalog.aggregate("heating").map { it.id }
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
                                val waterSources = GeneratedProviderCatalog.aggregate("water").map { it.id }
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
                                if (allEnabledByDefault || GeneratedProviderCatalog.aggregate("gas").any { it.id in enabledSources }) {
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

        val powerEnabled = allEnabledByDefault || GeneratedProviderCatalog.aggregate("power").any { it.id in enabledSources }
        val heatEnabled = allEnabledByDefault || GeneratedProviderCatalog.aggregate("heating").any { it.id in enabledSources }
        val waterEnabled = allEnabledByDefault || GeneratedProviderCatalog.aggregate("water").any { it.id in enabledSources }
        val gasEnabled = allEnabledByDefault || GeneratedProviderCatalog.aggregate("gas").any { it.id in enabledSources }

        updateResponsiveWidget(context, appWidgetManager, appWidgetId) { views, _ ->
            views.setOnClickPendingIntent(R.id.widget_root, refreshPending)
            views.setContentDescription(
                    R.id.widget_root,
                    context.getString(R.string.widget_accessibility_refresh)
            )
            views.setOnClickPendingIntent(R.id.section_power, clickPending)
            views.setOnClickPendingIntent(R.id.section_heat, clickPending)
            views.setOnClickPendingIntent(R.id.section_water, clickPending)
            views.setOnClickPendingIntent(R.id.section_gas, clickPending)
            views.setOnClickPendingIntent(R.id.count_power, clickPending)
            views.setOnClickPendingIntent(R.id.count_heat, clickPending)
            views.setOnClickPendingIntent(R.id.count_water, clickPending)
            views.setOnClickPendingIntent(R.id.count_gas, clickPending)

            views.setTextViewText(R.id.widget_address_name, addressName)
            views.setContentDescription(
                    R.id.widget_address_name,
                    context.getString(R.string.widget_accessibility_address, addressName)
            )
            views.setTextViewText(R.id.widget_updated, updatedAt)
            views.setContentDescription(
                    R.id.widget_updated,
                    context.getString(R.string.widget_accessibility_updated, updatedAt)
            )
            views.setViewVisibility(R.id.widget_updated, View.VISIBLE)

            views.setTextViewText(R.id.count_power, powerCount)
            views.setTextViewText(R.id.count_heat, heatCount)
            views.setTextViewText(R.id.count_water, waterCount)
            views.setTextViewText(R.id.count_gas, gasCount)
            // Quiet zero counts while retaining utility colors for active outages and status markers.
            views.setTextColor(R.id.count_power, context.getColor(if (powerCount == "0") R.color.widget_text_label else R.color.utility_power))
            views.setTextColor(R.id.count_heat, context.getColor(if (heatCount == "0") R.color.widget_text_label else R.color.utility_heat))
            views.setTextColor(R.id.count_water, context.getColor(if (waterCount == "0") R.color.widget_text_label else R.color.utility_water))
            views.setTextColor(R.id.count_gas, context.getColor(if (gasCount == "0") R.color.widget_text_label else R.color.utility_gas))
            setCountAccessibility(context, views, R.id.count_power, R.string.label_power, powerCount, addressName, updatedAt, powerEnabled)
            setCountAccessibility(context, views, R.id.count_heat, R.string.label_heat, heatCount, addressName, updatedAt, heatEnabled)
            setCountAccessibility(context, views, R.id.count_water, R.string.label_water, waterCount, addressName, updatedAt, waterEnabled)
            setCountAccessibility(context, views, R.id.count_gas, R.string.label_gas, gasCount, addressName, updatedAt, gasEnabled)
            applyAllTheme(views, powerEnabled, heatEnabled, waterEnabled, gasEnabled)
        }
    }

    private fun updateResponsiveWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int,
            render: (RemoteViews, Boolean) -> Unit
    ) {
        // Provisional thresholds: the roomier four-item row needs about 56dp per utility.
        val compactWidthDp = 110f
        val roomyWidthDp = 230f
        val widgetHeightDp = 48f
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val compact = RemoteViews(context.packageName, R.layout.widget_all_outage)
            val roomy = RemoteViews(context.packageName, R.layout.widget_all_outage_roomy)
            render(compact, false)
            render(roomy, true)
            appWidgetManager.updateAppWidget(
                    appWidgetId,
                    RemoteViews(mapOf(
                            SizeF(compactWidthDp, widgetHeightDp) to compact,
                            SizeF(roomyWidthDp, widgetHeightDp) to roomy
                    ))
            )
            return
        }

        // Use the minimum reported width so a portrait width range cannot select a clipped row.
        val options = appWidgetManager.getAppWidgetOptions(appWidgetId)
        val minWidth = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, compactWidthDp.toInt())
        val roomy = minWidth >= roomyWidthDp.toInt()
        val layoutId = if (roomy) R.layout.widget_all_outage_roomy else R.layout.widget_all_outage
        val views = RemoteViews(context.packageName, layoutId)
        render(views, roomy)
        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    private fun setCountAccessibility(
            context: Context,
            views: RemoteViews,
            countViewId: Int,
            labelResId: Int,
            count: String,
            addressName: String,
            updatedAt: String,
            enabled: Boolean
    ) {
        val label = context.getString(labelResId)
        val description = when {
            addressName == context.getString(R.string.msg_no_address) ->
                context.getString(R.string.widget_accessibility_count_no_address, label)
            !enabled -> context.getString(R.string.widget_accessibility_count_disabled, label, addressName)
            count == "–" -> context.getString(R.string.widget_accessibility_count_loading, label, addressName)
            count == "!" -> context.getString(R.string.widget_accessibility_count_unavailable, label, addressName, updatedAt)
            else -> context.getString(R.string.widget_accessibility_count, label, count, addressName, updatedAt)
        }
        views.setContentDescription(countViewId, description)
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
