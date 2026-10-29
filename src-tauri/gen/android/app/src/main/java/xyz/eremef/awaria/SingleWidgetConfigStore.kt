package xyz.eremef.awaria

import android.content.Context
import androidx.core.content.edit

enum class WidgetAddressMode { FOLLOW_PRIMARY, EXPLICIT }

data class SingleWidgetConfiguration(
    val providerId: String,
    val addressMode: WidgetAddressMode,
    val addressId: String?,
    val generation: Long
)

sealed class WidgetAddressResolution {
    data class Ready(val address: WidgetSettings) : WidgetAddressResolution()
    object NoActiveAddress : WidgetAddressResolution()
    object AddressMissing : WidgetAddressResolution()
    object AddressInactive : WidgetAddressResolution()
    object AddressAmbiguous : WidgetAddressResolution()
}

/** Per-appWidgetId preferences for the configurable single-provider widget. */
object SingleWidgetConfigStore {
    private const val PREFS_NAME = "xyz.eremef.awaria.WidgetPrefs"
    private const val CONFIG_PREFIX = "single_config_"
    private const val CACHE_PREFIX = "single_cache_"
    private const val GENERATION_KEY = "single_config_generation"

    private fun configKey(widgetId: Int, field: String) = "$CONFIG_PREFIX${widgetId}_$field"
    private fun cacheKey(widgetId: Int, field: String) = "$CACHE_PREFIX${widgetId}_$field"

    fun get(context: Context, widgetId: Int): SingleWidgetConfiguration? {
        if (widgetId == android.appwidget.AppWidgetManager.INVALID_APPWIDGET_ID) return null
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val providerId = prefs.getString(configKey(widgetId, "provider"), null) ?: return null
        val addressMode = runCatching {
            WidgetAddressMode.valueOf(prefs.getString(configKey(widgetId, "address_mode"), null)
                ?: WidgetAddressMode.FOLLOW_PRIMARY.name)
        }.getOrDefault(WidgetAddressMode.FOLLOW_PRIMARY)
        return SingleWidgetConfiguration(
            providerId = providerId,
            addressMode = addressMode,
            addressId = prefs.getString(configKey(widgetId, "address_id"), null),
            generation = prefs.getLong(configKey(widgetId, "generation"), 0L)
        )
    }

    /** Save the complete selection together and invalidate data associated with the old selection. */
    fun save(
        context: Context,
        widgetId: Int,
        providerId: String,
        addressMode: WidgetAddressMode,
        addressId: String? = null
    ): SingleWidgetConfiguration = synchronized(this) {
        require(widgetId != android.appwidget.AppWidgetManager.INVALID_APPWIDGET_ID)
        require(SingleProviderCatalog.find(providerId) != null) { "Unknown provider: $providerId" }
        require(addressMode != WidgetAddressMode.EXPLICIT || !addressId.isNullOrBlank()) {
            "An explicit address must have an ID"
        }
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val previousGeneration = get(context, widgetId)?.generation ?: 0L
        val nextGeneration = maxOf(prefs.getLong(GENERATION_KEY, 0L), previousGeneration) + 1L
        val next = SingleWidgetConfiguration(
            providerId = providerId,
            addressMode = addressMode,
            addressId = if (addressMode == WidgetAddressMode.EXPLICIT) addressId else null,
            generation = nextGeneration
        )
        prefs.edit {
            putString(configKey(widgetId, "provider"), next.providerId)
            putString(configKey(widgetId, "address_mode"), next.addressMode.name)
            if (next.addressId == null) remove(configKey(widgetId, "address_id"))
            else putString(configKey(widgetId, "address_id"), next.addressId)
            putLong(configKey(widgetId, "generation"), next.generation)
            putLong(GENERATION_KEY, next.generation)
            removeCache(this, widgetId)
        }
        next
    }

    fun clear(context: Context, widgetId: Int) = synchronized(this) {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        prefs.edit {
            remove(configKey(widgetId, "provider"))
            remove(configKey(widgetId, "address_mode"))
            remove(configKey(widgetId, "address_id"))
            remove(configKey(widgetId, "generation"))
            removeCache(this, widgetId)
        }
    }

    /** Remap a restored widget only when the destination ID has no unrelated saved selection. */
    fun remap(context: Context, oldId: Int, newId: Int): Boolean =
        remapAll(context, intArrayOf(oldId), intArrayOf(newId))

    /** Snapshot every source before writing so overlapping old/new IDs cannot clobber each other. */
    fun remapAll(context: Context, oldIds: IntArray, newIds: IntArray): Boolean = synchronized(this) {
        if (oldIds.isEmpty() || oldIds.size != newIds.size) return false
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val pairs = oldIds.indices.map { oldIds[it] to newIds[it] }
            .filter { (oldId, newId) ->
                oldId != newId &&
                    oldId != android.appwidget.AppWidgetManager.INVALID_APPWIDGET_ID &&
                    newId != android.appwidget.AppWidgetManager.INVALID_APPWIDGET_ID
            }
        if (pairs.isEmpty()) return false
        val mappedOldIds = pairs.map { it.first }.toSet()
        if (pairs.any { (_, newId) -> newId !in mappedOldIds && get(context, newId) != null }) return false
        val snapshots = pairs.mapNotNull { (oldId, newId) ->
            val config = get(context, oldId) ?: return@mapNotNull null
            Triple(oldId to newId, config, getCache(context, oldId, config))
        }
        if (snapshots.isEmpty()) return false
        prefs.edit {
            snapshots.forEach { (pair, _, _) ->
                val (oldId, _) = pair
                remove(configKey(oldId, "provider"))
                remove(configKey(oldId, "address_mode"))
                remove(configKey(oldId, "address_id"))
                remove(configKey(oldId, "generation"))
                removeCache(this, oldId)
            }
            snapshots.forEach { (pair, config, cache) ->
                val (_, newId) = pair
                putString(configKey(newId, "provider"), config.providerId)
                putString(configKey(newId, "address_mode"), config.addressMode.name)
                if (config.addressId == null) remove(configKey(newId, "address_id"))
                else putString(configKey(newId, "address_id"), config.addressId)
                putLong(configKey(newId, "generation"), config.generation)
                if (cache != null) putCache(this, newId, config.generation, cache)
            }
        }
        true
    }

    fun saveCache(
        context: Context,
        widgetId: Int,
        expectedGeneration: Long,
        addressKey: String,
        count: String,
        updatedAt: String
    ): Boolean = synchronized(this) {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        if (get(context, widgetId)?.generation != expectedGeneration) return false
        prefs.edit {
            putLong(cacheKey(widgetId, "generation"), expectedGeneration)
            putString(cacheKey(widgetId, "address"), addressKey)
            putString(cacheKey(widgetId, "count"), count)
            putString(cacheKey(widgetId, "updated"), updatedAt)
        }
        return get(context, widgetId)?.generation == expectedGeneration
    }

    fun getCache(
        context: Context,
        widgetId: Int,
        configuration: SingleWidgetConfiguration,
        addressKey: String? = null
    ): Pair<String, String>? {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        if (prefs.getLong(cacheKey(widgetId, "generation"), -1L) != configuration.generation) return null
        if (addressKey != null && prefs.getString(cacheKey(widgetId, "address"), null) != addressKey) return null
        val count = prefs.getString(cacheKey(widgetId, "count"), null) ?: return null
        val updated = prefs.getString(cacheKey(widgetId, "updated"), null) ?: return null
        return count to updated
    }

    /** Run a synchronous render only while this widget still owns the fetched configuration. */
    fun renderIfCurrent(
        context: Context,
        widgetId: Int,
        expectedGeneration: Long,
        render: () -> Unit
    ): Boolean = synchronized(this) {
        if (get(context, widgetId)?.generation != expectedGeneration) return false
        render()
        true
    }

    fun renderIfUnconfigured(context: Context, widgetId: Int, render: () -> Unit): Boolean = synchronized(this) {
        if (get(context, widgetId) != null) return false
        render()
        true
    }

    private fun putCache(
        editor: android.content.SharedPreferences.Editor,
        widgetId: Int,
        generation: Long,
        cache: Triple<String, String, String>
    ) {
        editor.putLong(cacheKey(widgetId, "generation"), generation)
        editor.putString(cacheKey(widgetId, "address"), cache.first)
        editor.putString(cacheKey(widgetId, "count"), cache.second)
        editor.putString(cacheKey(widgetId, "updated"), cache.third)
    }

    private fun removeCache(editor: android.content.SharedPreferences.Editor, widgetId: Int) {
        editor.remove(cacheKey(widgetId, "generation"))
        editor.remove(cacheKey(widgetId, "address"))
        editor.remove(cacheKey(widgetId, "count"))
        editor.remove(cacheKey(widgetId, "updated"))
    }

    private fun getCache(context: Context, widgetId: Int, configuration: SingleWidgetConfiguration): Triple<String, String, String>? {
        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        if (prefs.getLong(cacheKey(widgetId, "generation"), -1L) != configuration.generation) return null
        val address = prefs.getString(cacheKey(widgetId, "address"), null) ?: return null
        val count = prefs.getString(cacheKey(widgetId, "count"), null) ?: return null
        val updated = prefs.getString(cacheKey(widgetId, "updated"), null) ?: return null
        return Triple(address, count, updated)
    }
}

object SingleWidgetAddressResolver {
    /** Matches the address key used by the existing aggregate widgets; duplicates are reported. */
    fun addressId(address: WidgetSettings): String = "${address.cityId}-${address.streetId}-${address.houseNo}"

    fun resolve(
        configuration: SingleWidgetConfiguration,
        addresses: List<WidgetSettings>
    ): WidgetAddressResolution {
        if (configuration.addressMode == WidgetAddressMode.FOLLOW_PRIMARY) {
            return addresses.firstOrNull { it.isPrimary && it.isActive }
                ?.let(WidgetAddressResolution::Ready)
                ?: addresses.firstOrNull { it.isActive }
                    ?.let(WidgetAddressResolution::Ready)
                ?: WidgetAddressResolution.NoActiveAddress
        }
        val id = configuration.addressId ?: return WidgetAddressResolution.AddressMissing
        val matches = addresses.filter { addressId(it) == id }
        return when {
            matches.isEmpty() -> WidgetAddressResolution.AddressMissing
            matches.size > 1 -> WidgetAddressResolution.AddressAmbiguous
            !matches.single().isActive -> WidgetAddressResolution.AddressInactive
            else -> WidgetAddressResolution.Ready(matches.single())
        }
    }
}
