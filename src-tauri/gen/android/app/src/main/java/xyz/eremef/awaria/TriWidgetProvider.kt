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

class TriWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_TRI"
    override val primaryColorRes: Int = R.color.widget_text_primary
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "status"
    override val sourceKey: String = "tri_status"

    override fun showLoadingPlaceholder(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
    ) {
        val views = RemoteViews(context.packageName, R.layout.widget_tri_outage)
        val refreshIntent = Intent(context, this::class.java).apply {
            action = refreshAction
            putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
        }
        val pending = PendingIntent.getBroadcast(
                context, appWidgetId, refreshIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        views.setOnClickPendingIntent(R.id.widget_root, pending)
        views.setTextViewText(R.id.count_power, "–")
        views.setTextViewText(R.id.count_heat, "–")
        views.setTextViewText(R.id.count_water, "–")
        views.setTextViewText(R.id.widget_updated, context.getString(R.string.msg_updating))
        views.setTextViewText(R.id.widget_address_name, "")
        views.setTextViewText(R.id.label_power, context.getString(R.string.label_power))
        views.setTextViewText(R.id.label_heat, context.getString(R.string.label_heat))
        views.setTextViewText(R.id.label_water, context.getString(R.string.label_water))
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
            appWidgetId: Int
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

        val theme = allSettings?.firstOrNull()?.theme ?: "system"
        val dark = isDarkMode(context, theme)

        val enabledSources = getEnabledSources(fullJson)
        val allEnabledByDefault = fullJson?.has("enabledSources") == false

        var powerCount = "–"
        var heatCount = "–"
        var waterCount = "–"
        var totalOutages = 0

        if (selectedAddress != null) {
            val settingsList = listOf(selectedAddress)
            val hash = calculateHash(settingsList)

            if (!WidgetUtils.isNetworkAvailable(context)) {
                powerCount = "!"
                heatCount = "!"
                waterCount = "!"
            } else {
                try {
                    coroutineScope {
                        val settingsJson =
                                WidgetUtils.serializeSettingsForRust(settingsList, fullJson)
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
                                                        "TriWidget",
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
                                                        "TriWidget",
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
                            val waterSources = listOf("mpwik_wroclaw", "mpwik_warszawa", "wmk", "aquanet", "katowickie_wodociagi", "zwik_lodz", "pwik_kalisz", "pwik_czestochowa", "wodociagi_plockie", "gdanskie_wodociagi")
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
                                                        "TriWidget",
                                                        "Failed to fetch $source: ${e.message}"
                                                )
                                                0
                                            }
                                        }
                                    }
                                    .awaitAll()
                                    .sum()
                        }

                        val resP = p.await()
                        val resH = h.await()
                        val resW = w.await()

                        powerCount = resP.toString()
                        heatCount = resH.toString()
                        waterCount = resW.toString()
                        totalOutages = resP + resH + resW
                    }
                } catch (e: Exception) {
                    Log.e("TriWidget", "Error fetching counts", e)
                    powerCount = "!"
                    heatCount = "!"
                    waterCount = "!"
                }
            }
        }

        val updatedAt =
                if (!WidgetUtils.isNetworkAvailable(context)) {
                    getTranslation(context, "offline")
                } else {
                    SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date())
                }
        val addressName =
                if (selectedAddress != null) {
                    if (selectedAddress.name.isNotEmpty()) selectedAddress.name
                    else
                            "${selectedAddress.cityName}, ${selectedAddress.streetName} ${selectedAddress.houseNo}"
                } else {
                    getTranslation(context, "no_address")
                }

        val views = RemoteViews(context.packageName, R.layout.widget_tri_outage)

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

        // Texts
        views.setTextViewText(R.id.widget_address_name, addressName)
        views.setTextViewText(R.id.widget_updated, updatedAt)
        views.setTextViewText(R.id.count_power, powerCount)
        views.setTextViewText(R.id.count_heat, heatCount)
        views.setTextViewText(R.id.count_water, waterCount)

        // Labels
        views.setTextViewText(R.id.label_power, getTranslation(context, "power"))
        views.setTextViewText(R.id.label_heat, getTranslation(context, "heat"))
        views.setTextViewText(R.id.label_water, getTranslation(context, "water"))

        // Check enabled utilities


        val powerEnabled =
                allEnabledByDefault ||
                        listOf("tauron", "stoen", "energa", "enea", "pge").any {
                            it in enabledSources
                        }
        val heatEnabled =
                allEnabledByDefault || listOf("fortum", "tauron_heat", "veolia_warszawa", "veolia_poznan", "veolia_lodz", "gpec").any { it in enabledSources }
        val waterEnabled =
                allEnabledByDefault || listOf("mpwik_wroclaw", "mpwik_warszawa", "wmk", "aquanet", "katowickie_wodociagi", "zwik_lodz", "pwik_kalisz", "pwik_czestochowa", "wodociagi_plockie", "gdanskie_wodociagi").any { it in enabledSources }

        // Theme
        applyTriTheme(context, views, theme, dark, powerEnabled, heatEnabled, waterEnabled)

        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    private fun applyTriTheme(
            context: Context,
            views: RemoteViews,
            themeSetting: String,
            dark: Boolean,
            powerEnabled: Boolean,
            heatEnabled: Boolean,
            waterEnabled: Boolean
    ) {
        // If system theme is selected, the XML handles background and generic label colors
        // automatically.
        // We only explicitly set them here to support manual theme overrides.

        if (themeSetting != "system") {
            val bgRes =
                    if (dark) R.drawable.widget_background_dark else R.drawable.widget_background
            if (bgRes != 0) {
                views.setInt(R.id.widget_root, "setBackgroundResource", bgRes)
            }

            val labelColor = context.getColor(R.color.widget_text_label)
            val updatedColor = context.getColor(R.color.widget_text_updated)

            views.setTextColor(R.id.widget_address_name, updatedColor)
            views.setTextColor(R.id.widget_updated, updatedColor)
            views.setTextColor(R.id.label_power, labelColor)
            views.setTextColor(R.id.label_heat, labelColor)
            views.setTextColor(R.id.label_water, labelColor)
        }

        // Utility Colors Pull from theme-aware resources.
        // We set these in code because they can be tinted/forced per address logic,
        // but we use the resource ID to let context resolve it.
        val colorPower = context.getColor(R.color.utility_power)
        val colorHeat = context.getColor(R.color.utility_heat)
        val colorWater = context.getColor(R.color.utility_water)

        views.setTextColor(R.id.count_power, colorPower)
        views.setTextColor(R.id.count_heat, colorHeat)
        views.setTextColor(R.id.count_water, colorWater)

        views.setInt(R.id.icon_power, "setColorFilter", colorPower)
        views.setInt(R.id.icon_heat, "setColorFilter", colorHeat)
        views.setInt(R.id.icon_water, "setColorFilter", colorWater)

        // Gray out disabled utilities
        val disabledAlpha = 0.3f
        views.setFloat(R.id.section_power, "setAlpha", if (powerEnabled) 1.0f else disabledAlpha)
        views.setFloat(R.id.section_heat, "setAlpha", if (heatEnabled) 1.0f else disabledAlpha)
        views.setFloat(R.id.section_water, "setAlpha", if (waterEnabled) 1.0f else disabledAlpha)
    }
}
