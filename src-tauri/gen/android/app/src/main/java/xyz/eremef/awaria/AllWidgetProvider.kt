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

class AllWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ALL"
    override val primaryColorRes: Int = R.color.widget_text_primary
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "status"
    override val sourceKey: String = "all_status"

    override suspend fun updateWidget(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetId: Int
    ) {
        val allSettings = loadSettings(context)
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

        val language = allSettings?.firstOrNull()?.language ?: "system"
        val theme = allSettings?.firstOrNull()?.theme ?: "system"
        val dark = isDarkMode(context, theme)

        var powerCount = "–"
        var heatCount = "–"
        var waterCount = "–"
        var gasCount = "–"
        var totalOutages = 0

        if (selectedAddress != null) {
            val settingsList = listOf(selectedAddress)
            val hash = calculateHash(settingsList)

            try {
                coroutineScope {
                    val settingsJson = WidgetUtils.serializeSettingsForRust(settingsList)
                    val p = async {
                        val sources = listOf("tauron", "stoen", "energa", "enea", "pge")
                        sources.map { source ->
                            async {
                                try {
                                    ProviderCache.getOrFetch(source, hash) {
                                        WidgetUtils.fetchCountFromRust(context, source, settingsJson)
                                    }
                                } catch (e: Exception) {
                                    Log.w("AllWidget", "Failed to fetch $source: ${e.message}")
                                    0
                                }
                            }
                        }.awaitAll().sum()
                    }
                    val h = async {
                        try {
                                ProviderCache.getOrFetch("fortum", hash) {
                                    WidgetUtils.fetchCountFromRust(context, "fortum", settingsJson)
                                }
                        } catch (e: Exception) {
                            0
                        }
                    }
                    val w = async {
                        try {
                                ProviderCache.getOrFetch("water", hash) {
                                    WidgetUtils.fetchCountFromRust(context, "water", settingsJson)
                                }
                        } catch (e: Exception) {
                            0
                        }
                    }
                    val g = async {
                        try {
                                ProviderCache.getOrFetch("psg", hash) {
                                    WidgetUtils.fetchCountFromRust(context, "psg", settingsJson)
                                }
                        } catch (e: Exception) {
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
                    gasCount = resG.toString()
                    totalOutages = resP + resH + resW + resG
                }
            } catch (e: Exception) {
                Log.e("AllWidget", "Error fetching counts", e)
                powerCount = "!"
                heatCount = "!"
                waterCount = "!"
                gasCount = "!"
            }
        }

        val updatedAt = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date())
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
        views.setTextViewText(R.id.label_power, getTranslation(context, "power"))
        views.setTextViewText(R.id.label_heat, getTranslation(context, "heat"))
        views.setTextViewText(R.id.label_water, getTranslation(context, "water"))
        views.setTextViewText(R.id.label_gas, getTranslation(context, "gas"))

        // Theme
        applyAllTheme(context, views, theme, dark)

        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    private fun applyAllTheme(context: Context, views: RemoteViews, themeSetting: String, dark: Boolean) {
        if (themeSetting != "system") {
            val bgRes = if (dark) R.drawable.widget_background_dark else R.drawable.widget_background
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
            views.setTextColor(R.id.label_gas, labelColor)
        }

        val colorPower = context.getColor(R.color.utility_power)
        val colorHeat = context.getColor(R.color.utility_heat)
        val colorWater = context.getColor(R.color.utility_water)
        val colorGas = context.getColor(R.color.utility_gas)

        views.setTextColor(R.id.count_power, colorPower)
        views.setTextColor(R.id.count_heat, colorHeat)
        views.setTextColor(R.id.count_water, colorWater)
        views.setTextColor(R.id.count_gas, colorGas)

        views.setInt(R.id.icon_power, "setColorFilter", colorPower)
        views.setInt(R.id.icon_heat, "setColorFilter", colorHeat)
        views.setInt(R.id.icon_water, "setColorFilter", colorWater)
        views.setInt(R.id.icon_gas, "setColorFilter", colorGas)
    }
}
