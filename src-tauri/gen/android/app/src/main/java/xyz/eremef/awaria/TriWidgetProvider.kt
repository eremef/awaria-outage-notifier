package xyz.eremef.awaria

import xyz.eremef.awaria.R
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.util.Log
import android.widget.RemoteViews
import kotlinx.coroutines.*
import java.text.SimpleDateFormat
import java.util.*

class TriWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_TRI"
    override val lightPrimary: String = "#D9006C"
    override val darkPrimary: String = "#FF4DA6"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "status"
    override val sourceKey: String = "tri_status"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return 0
    }

    override suspend fun updateWidget(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int
    ) {
        val allSettings = loadSettings(context)
        val primaryAddress = allSettings?.find { it.isPrimary } ?: allSettings?.firstOrNull { it.isActive }
        
        val customAddressId = getStoredAddressId(context, appWidgetId)
        val selectedAddress = if (customAddressId != null && allSettings != null) {
            allSettings.find { "${it.cityId}-${it.streetId}-${it.houseNo}" == customAddressId } ?: primaryAddress
        } else {
            primaryAddress
        }

        val language = allSettings?.firstOrNull()?.language ?: "system"
        val theme = allSettings?.firstOrNull()?.theme ?: "system"
        val dark = isDarkMode(context, theme)

        var powerCount = "–"
        var heatCount = "–"
        var waterCount = "–"
        var totalOutages = 0

        if (selectedAddress != null) {
            val settingsList = listOf(selectedAddress)
            val hash = calculateHash(settingsList)
            
            try {
                coroutineScope {
                    val p = async {
                        val counts = listOf(
                            async { try { ProviderCache.getOrFetch("tauron", hash) { TauronProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } },
                            async { try { ProviderCache.getOrFetch("stoen", hash) { StoenProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } },
                            async { try { ProviderCache.getOrFetch("energa", hash) { EnergaProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } },
                            async { try { ProviderCache.getOrFetch("enea", hash) { EneaProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } },
                            async { try { ProviderCache.getOrFetch("pge", hash) { PgeProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } }
                        )
                        counts.awaitAll().sum()
                    }
                    val h = async { try { ProviderCache.getOrFetch("fortum", hash) { FortumProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } }
                    val w = async { try { ProviderCache.getOrFetch("water", hash) { MpwikProvider().fetchCount(context, settingsList) } } catch(e:Exception) { 0 } }

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

        val updatedAt = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date())
        val prefsAddressId = getStoredAddressId(context, appWidgetId)
        val addressName = if (selectedAddress != null) {
            if (selectedAddress.name.isNotEmpty()) selectedAddress.name 
            else "${selectedAddress.cityName}, ${selectedAddress.streetName} ${selectedAddress.houseNo}"
        } else {
            getTranslation(context, "no_address")
        }

        val views = RemoteViews(context.packageName, R.layout.widget_tri_outage)
        
        // Clicks
        val refreshIntent = Intent(context, this::class.java).apply { action = refreshAction }
        val refreshPending = PendingIntent.getBroadcast(context, 0, refreshIntent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        
        val clickPending = if (totalOutages > 0) {
            val launchIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)?.apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            }
            if (launchIntent != null) PendingIntent.getActivity(context, 0, launchIntent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
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

        // Theme
        applyTriTheme(views, dark)

        appWidgetManager.updateAppWidget(appWidgetId, views)
    }

    private fun applyTriTheme(views: RemoteViews, dark: Boolean) {
        val labelColor = Color.parseColor(if (dark) "#A0A0A0" else "#666666")
        val updatedColor = Color.parseColor(if (dark) "#777777" else "#999999")
        
        // Background
        val bgRes = if (dark) R.drawable.widget_background_dark else R.drawable.widget_background
        if (bgRes != 0) {
            views.setInt(R.id.widget_root, "setBackgroundResource", bgRes)
        }

        views.setTextColor(R.id.widget_address_name, labelColor)
        views.setTextColor(R.id.widget_updated, updatedColor)
        views.setTextColor(R.id.label_power, labelColor)
        views.setTextColor(R.id.label_heat, labelColor)
        views.setTextColor(R.id.label_water, labelColor)
        
        // Colors for counts
        views.setTextColor(R.id.count_power, Color.parseColor(if (dark) "#FF4DA6" else "#D9006C"))
        views.setTextColor(R.id.count_heat, Color.parseColor(if (dark) "#00C86B" else "#00A859"))
        views.setTextColor(R.id.count_water, Color.parseColor(if (dark) "#4DA6FF" else "#0077D9"))
        
        // Icons
        val iconPower = Color.parseColor(if (dark) "#FF4DA6" else "#D9006C")
        val iconHeat = Color.parseColor(if (dark) "#00C86B" else "#00A859")
        val iconWater = Color.parseColor(if (dark) "#4DA6FF" else "#0077D9")

        views.setInt(R.id.icon_power, "setColorFilter", iconPower)
        views.setInt(R.id.icon_heat, "setColorFilter", iconHeat)
        views.setInt(R.id.icon_water, "setColorFilter", iconWater)
    }
}
