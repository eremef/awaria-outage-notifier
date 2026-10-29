package xyz.eremef.awaria

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import java.text.Collator
import java.text.Normalizer
import java.util.Locale

enum class WidgetUtility(val labelRes: Int, val iconRes: Int, val largeLayout: Int, val smallLayout: Int) {
    ELECTRICITY(R.string.label_power, R.drawable.ic_electricity, R.layout.widget_outage_power, R.layout.widget_outage_power_small),
    HEAT(R.string.label_heat, R.drawable.ic_heating, R.layout.widget_outage_heat, R.layout.widget_outage_heat_small),
    WATER(R.string.label_water, R.drawable.ic_water, R.layout.widget_outage_water, R.layout.widget_outage_water_small),
    GAS(R.string.label_gas, R.drawable.ic_gas, R.layout.widget_outage_gas, R.layout.widget_outage_gas_small)
}

data class SingleProviderEntry(
    val sourceId: String,
    val utility: WidgetUtility,
    val legacyComponentClass: String,
    val aliases: List<String> = emptyList(),
    val location: String = "",
    private val displayNamePl: String,
    private val displayNameEn: String
) {
    fun displayName(context: Context): String = localizedProviderName(context, displayNamePl, displayNameEn)
}

fun GeneratedProviderMetadata.displayName(context: Context): String =
    localizedProviderName(context, displayNamePl, displayNameEn)

private fun localizedProviderName(context: Context, polish: String, english: String): String {
    val locales = context.resources.configuration.locales
    for (index in 0 until locales.size()) {
        when (locales[index].language) {
            "pl" -> return polish
            "en" -> return english
        }
    }
    return polish
}

data class CatalogMatches(val enabled: List<SingleProviderEntry>, val other: List<SingleProviderEntry>)

/** Selection/search behavior backed by the generated provider metadata. */
object SingleProviderCatalog {
    val entries: List<SingleProviderEntry> = GeneratedProviderCatalog.entries
        .asSequence()
        .filter { it.singleWidget }
        .sortedWith(compareBy<GeneratedProviderMetadata> { it.androidOrder ?: Int.MAX_VALUE }.thenBy { it.id })
        .map { provider ->
            SingleProviderEntry(
                sourceId = provider.id,
                utility = provider.utility.toWidgetUtility(),
                legacyComponentClass = requireNotNull(provider.legacyComponentClass) {
                    "Single-widget provider ${provider.id} has no legacy component binding"
                },
                aliases = provider.searchAliases,
                location = provider.location.orEmpty(),
                displayNamePl = provider.displayNamePl,
                displayNameEn = provider.displayNameEn
            )
        }
        .toList()

    val bySourceId: Map<String, SingleProviderEntry> = entries.associateBy { it.sourceId }

    fun find(sourceId: String?): SingleProviderEntry? = sourceId?.let(bySourceId::get)

    fun forLegacyComponent(className: String?): SingleProviderEntry? {
        if (className.isNullOrBlank()) return null
        val simpleName = className.substringAfterLast('.')
        return entries.firstOrNull { it.legacyComponentClass == simpleName }
    }

    /** Inspect installed manifest registrations so receiver/catalog drift stays observable. */
    fun legacyManifestComponents(context: Context): Set<String> {
        val query = Intent(android.appwidget.AppWidgetManager.ACTION_APPWIDGET_UPDATE)
            .setPackage(context.packageName)
        @Suppress("DEPRECATION")
        val receivers = context.packageManager.queryBroadcastReceivers(query, PackageManager.GET_META_DATA)
        return receivers.mapNotNull { resolveInfo ->
            val receiver = resolveInfo.activityInfo ?: return@mapNotNull null
            val metadata = receiver.metaData ?: return@mapNotNull null
            if (metadata.getInt("android.appwidget.provider", 0) != R.xml.widget_single_info) return@mapNotNull null
            receiver.name.substringAfterLast('.')
        }.toSet()
    }

    fun manifestParityIssues(context: Context): List<String> {
        val expected = entries.map { it.legacyComponentClass }.toSet()
        val actual = legacyManifestComponents(context)
        return buildList {
            (expected - actual).sorted().forEach { add("Catalog component is not registered as a legacy widget: $it") }
            (actual - expected).sorted().forEach { add("Legacy single-widget receiver is missing from the catalog: $it") }
        }
    }

    /** Search both sections together; names, aliases, locations and stable IDs are indexed. */
    fun search(
        query: String,
        utilityFilter: WidgetUtility?,
        enabledSources: Set<String>,
        localizedNames: Map<String, String> = emptyMap()
    ): CatalogMatches {
        val tokens = normalize(query).split(Regex("\\s+")).filter(String::isNotBlank)
        val matches = entries.asSequence()
            .filter { utilityFilter == null || it.utility == utilityFilter }
            .filter { entry ->
                val indexed = normalize(
                    listOf(entry.sourceId, localizedNames[entry.sourceId].orEmpty(), entry.location)
                        .plus(entry.aliases)
                        .joinToString(" ")
                )
                tokens.all(indexed::contains)
            }
            .toList()
        val collator = Collator.getInstance(Locale.getDefault()).apply { strength = Collator.PRIMARY }
        val comparator = compareBy<SingleProviderEntry, String>(collator) {
            localizedNames[it.sourceId] ?: it.sourceId
        }.thenBy { it.sourceId }
        return CatalogMatches(
            enabled = matches.filter { it.sourceId in enabledSources }.sortedWith(comparator),
            other = matches.filterNot { it.sourceId in enabledSources }.sortedWith(comparator)
        )
    }

    fun normalize(value: String): String = Normalizer.normalize(value.replace('ł', 'l').replace('Ł', 'l'), Normalizer.Form.NFD)
        .filter { Character.getType(it) != Character.NON_SPACING_MARK.toInt() }
        .lowercase(Locale.ROOT)
}

private fun String.toWidgetUtility(): WidgetUtility = when (this) {
    "power" -> WidgetUtility.ELECTRICITY
    "heating" -> WidgetUtility.HEAT
    "water" -> WidgetUtility.WATER
    "gas" -> WidgetUtility.GAS
    else -> error("Unknown generated provider utility: $this")
}
