package xyz.eremef.awaria

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.util.Log
import android.widget.RemoteViews
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** The configurable provider used for new single-utility widgets. */
class SingleWidgetProvider : BaseWidgetProvider() {
    override val refreshAction = "xyz.eremef.awaria.ACTION_REFRESH_SINGLE_PROVIDER"
    override val primaryColorRes = R.color.utility_power
    override val iconResId = R.drawable.ic_electricity
    override val labelKey = "outages"
    // The configured source is read per widget; this sentinel is never sent to Rust.
    override val sourceKey = "single_provider"

    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        super.onDeleted(context, appWidgetIds)
        appWidgetIds.forEach { SingleWidgetConfigStore.clear(context, it) }
    }

    override fun onRestored(context: Context, oldWidgetIds: IntArray, newWidgetIds: IntArray) {
        super.onRestored(context, oldWidgetIds, newWidgetIds)
        SingleWidgetConfigStore.remapAll(context, oldWidgetIds, newWidgetIds)
        if (newWidgetIds.isNotEmpty()) onUpdate(context, AppWidgetManager.getInstance(context), newWidgetIds)
    }

    override fun showLoadingPlaceholder(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int
    ) {
        val configuration = SingleWidgetConfigStore.get(context, appWidgetId)
        val entry = SingleProviderCatalog.find(configuration?.providerId)
        if (configuration == null || entry == null) {
            showRepairPlaceholder(context, appWidgetManager, appWidgetId, configuration)
            return
        }
        SingleWidgetConfigStore.renderIfCurrent(context, appWidgetId, configuration.generation) {
            renderCatalogWidget(
                context, appWidgetManager, appWidgetId, entry,
                "–", context.getString(R.string.msg_updating), refreshAction
            )
        }
    }

    override suspend fun updateWidget(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        useCacheOnly: Boolean
    ) {
        updateConfiguredWidget(context, appWidgetManager, appWidgetId, useCacheOnly, addressRetryBudget = 1)
    }

    private suspend fun updateConfiguredWidget(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        useCacheOnly: Boolean,
        addressRetryBudget: Int
    ) {
        val configuration = SingleWidgetConfigStore.get(context, appWidgetId)
        val entry = SingleProviderCatalog.find(configuration?.providerId)
        if (configuration == null || entry == null) {
            showRepairPlaceholder(context, appWidgetManager, appWidgetId, configuration)
            return
        }

        val settingsResult = loadSettings(context, entry.sourceId)
        if (settingsResult == null) {
            renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_settings))
            return
        }
        val (addresses, fullJson) = settingsResult
        val sourceEnabled = addresses.firstOrNull()?.sourceEnabled ?: true
        if (!sourceEnabled) {
            renderState(context, appWidgetManager, appWidgetId, configuration, entry, "–", context.getString(R.string.msg_inactive))
            return
        }

        when (val resolution = SingleWidgetAddressResolver.resolve(configuration, addresses)) {
            WidgetAddressResolution.NoActiveAddress -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_no_active_address))
                return
            }
            WidgetAddressResolution.AddressMissing -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_address_missing))
                return
            }
            WidgetAddressResolution.AddressInactive -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_address_inactive))
                return
            }
            WidgetAddressResolution.AddressAmbiguous -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_address_ambiguous))
                return
            }
            is WidgetAddressResolution.Ready -> {
                updateForAddress(
                    context, appWidgetManager, appWidgetId, configuration, entry,
                    resolution.address, fullJson, useCacheOnly, addressRetryBudget
                )
            }
        }
    }

    private suspend fun updateForAddress(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        configuration: SingleWidgetConfiguration,
        entry: SingleProviderEntry,
        address: WidgetSettings,
        fullJson: org.json.JSONObject?,
        useCacheOnly: Boolean,
        addressRetryBudget: Int
    ) {
        val settings = listOf(address)
        val settingsJson = WidgetUtils.serializeSettingsForRust(settings, fullJson)
        val addressIdentity = SingleWidgetAddressResolver.addressId(address)
        val addressKey = "$addressIdentity:${settingsJson.hashCode()}"
        val cached = SingleWidgetConfigStore.getCache(context, appWidgetId, configuration, addressKey)
        var count: String
        var updatedAt: String
        var saveFetchedResult = false

        if (useCacheOnly) {
            if (cached != null) {
                count = cached.first
                updatedAt = cached.second
            } else {
                // A resize should only change the layout; the regular worker or a refresh tap
                // can populate a missing value later without starting an unexpected request.
                count = "–"
                updatedAt = context.getString(R.string.widget_no_cached_data)
            }
        } else if (!WidgetUtils.isNetworkAvailable(context)) {
            if (cached != null) {
                count = cached.first
                updatedAt = context.getString(R.string.widget_cached_offline, cached.second)
            } else {
                count = "!"
                updatedAt = context.getString(R.string.msg_offline)
            }
        } else {
            try {
                val hash = settingsJson.hashCode().toString()
                val total = ProviderCache.getOrFetch(entry.sourceId, hash) {
                    if (entry.sourceId == "psg") {
                        PsgWebViewFetcher.fetchCount(context, settings)
                    } else {
                        WidgetUtils.fetchCountFromRust(context, entry.sourceId, settingsJson)
                    }
                }
                if (total >= 0) {
                    count = total.toString()
                    updatedAt = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date())
                    saveFetchedResult = true
                } else {
                    count = "!"
                    updatedAt = context.getString(R.string.msg_error)
                    if (cached != null) {
                        count = cached.first
                        updatedAt = context.getString(R.string.widget_cached_error, cached.second)
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "Error fetching ${entry.sourceId} for widget $appWidgetId: ${e.message}", e)
                count = cached?.first ?: "!"
                updatedAt = cached?.let { context.getString(R.string.widget_cached_error, it.second) }
                    ?: context.getString(R.string.msg_error)
            }
        }

        // Settings can change while the network request is in flight. Re-resolve both provider
        // enablement and the address before allowing the fetched value to reach the widget.
        val latestSettings = loadSettings(context, entry.sourceId)
        if (latestSettings == null) {
            renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_settings))
            return
        }
        val latestAddresses = latestSettings.first
        val latestSourceEnabled = latestAddresses.firstOrNull()?.sourceEnabled ?: true
        if (!latestSourceEnabled) {
            renderState(context, appWidgetManager, appWidgetId, configuration, entry, "–", context.getString(R.string.msg_inactive))
            return
        }
        when (val latestResolution = SingleWidgetAddressResolver.resolve(configuration, latestAddresses)) {
            is WidgetAddressResolution.Ready -> {
                val latestSettingsJson = WidgetUtils.serializeSettingsForRust(
                    listOf(latestResolution.address), latestSettings.second
                )
                val latestAddressKey = "${SingleWidgetAddressResolver.addressId(latestResolution.address)}:${latestSettingsJson.hashCode()}"
                if (latestAddressKey != addressKey) {
                    if (addressRetryBudget > 0) {
                        updateConfiguredWidget(context, appWidgetManager, appWidgetId, false, addressRetryBudget - 1)
                    } else {
                        renderState(context, appWidgetManager, appWidgetId, configuration, entry, "–", context.getString(R.string.msg_updating))
                    }
                    return
                }
            }
            WidgetAddressResolution.NoActiveAddress -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_no_active_address))
                return
            }
            WidgetAddressResolution.AddressMissing -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_address_missing))
                return
            }
            WidgetAddressResolution.AddressInactive -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_address_inactive))
                return
            }
            WidgetAddressResolution.AddressAmbiguous -> {
                renderState(context, appWidgetManager, appWidgetId, configuration, entry, "?", context.getString(R.string.widget_setup_address_ambiguous))
                return
            }
        }

        // Hold the configuration generation stable through the final view/cache write.
        SingleWidgetConfigStore.renderIfCurrent(context, appWidgetId, configuration.generation) {
            if (saveFetchedResult) {
                SingleWidgetConfigStore.saveCache(
                    context, appWidgetId, configuration.generation, addressKey, count, updatedAt
                )
            }
            renderCatalogWidget(context, appWidgetManager, appWidgetId, entry, count, updatedAt, refreshAction)
        }
    }

    private fun renderState(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        configuration: SingleWidgetConfiguration,
        entry: SingleProviderEntry,
        count: String,
        message: String
    ) {
        val configureIntent = Intent(context, WidgetConfigActivity::class.java).apply {
            putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val configurePending = PendingIntent.getActivity(
            context, appWidgetId, configureIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        SingleWidgetConfigStore.renderIfCurrent(context, appWidgetId, configuration.generation) {
            renderCatalogWidget(
                context, appWidgetManager, appWidgetId, entry, count, message,
                refreshAction, configurePending
            )
        }
    }

    private fun showRepairPlaceholder(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        configuration: SingleWidgetConfiguration? = null
    ) {
        val views = RemoteViews(context.packageName, R.layout.widget_single_unconfigured)
        val configureIntent = Intent(context, WidgetConfigActivity::class.java).apply {
            putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pending = PendingIntent.getActivity(
            context, appWidgetId, configureIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        views.setOnClickPendingIntent(R.id.widget_root, pending)
        val render = { appWidgetManager.updateAppWidget(appWidgetId, views) }
        if (configuration == null) SingleWidgetConfigStore.renderIfUnconfigured(context, appWidgetId, render)
        else SingleWidgetConfigStore.renderIfCurrent(context, appWidgetId, configuration.generation, render)
    }
}
