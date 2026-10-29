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
    val nameRes: Int,
    val utility: WidgetUtility,
    val legacyComponentClass: String,
    val aliases: List<String> = emptyList(),
    val location: String = ""
) {
    fun displayName(context: Context): String = context.getString(nameRes)
}

data class CatalogMatches(val enabled: List<SingleProviderEntry>, val other: List<SingleProviderEntry>)

/** Explicit source-to-widget mapping. Aggregate widgets are deliberately not selectable here. */
object SingleProviderCatalog {
    val entries: List<SingleProviderEntry> = listOf(
        provider("tauron", R.string.provider_tauron, WidgetUtility.ELECTRICITY, "TauronWidgetProvider", "TAURON", "Polska"),
        provider("stoen", R.string.provider_stoen, WidgetUtility.ELECTRICITY, "StoenWidgetProvider", "E.ON", "Warszawa"),
        provider("enea", R.string.provider_enea, WidgetUtility.ELECTRICITY, "EneaWidgetProvider", "ENEA", "Polska"),
        provider("energa", R.string.provider_energa, WidgetUtility.ELECTRICITY, "EnergaWidgetProvider", "ENERGA", "Polska"),
        provider("pge", R.string.provider_pge, WidgetUtility.ELECTRICITY, "PgeWidgetProvider", "PGE", "Polska"),
        provider("enea_cieplo", R.string.provider_enea_cieplo, WidgetUtility.HEAT, "EneaCieploWidgetProvider", "ENEA Ciepło", "Bydgoszcz"),
        provider("eco_sa", R.string.provider_eco_sa, WidgetUtility.HEAT, "EcoSaWidgetProvider", "ECO", "Opole"),
        provider("energa_cieplo_kaliskie", R.string.provider_energa_cieplo_kaliskie, WidgetUtility.HEAT, "EnergaCieploKaliskieWidgetProvider", "Energa Ciepło", "Kalisz"),
        provider("fortum", R.string.provider_fortum, WidgetUtility.HEAT, "FortumWidgetProvider", "Fortum", "Polska"),
        provider("tauron_heat", R.string.provider_tauron_heat, WidgetUtility.HEAT, "TauronHeatWidgetProvider", "Tauron Ciepło", "Polska"),
        provider("therma_bielsko", R.string.provider_therma_bielsko, WidgetUtility.HEAT, "ThermaBielskoWidgetProvider", "Therma", "Bielsko-Biała"),
        provider("veolia_warszawa", R.string.provider_veolia_warszawa, WidgetUtility.HEAT, "VeoliaWidgetProvider", "Veolia", "Warszawa"),
        provider("veolia_poznan", R.string.provider_veolia_poznan, WidgetUtility.HEAT, "VeoliaPoznanWidgetProvider", "Veolia", "Poznań"),
        provider("veolia_lodz", R.string.provider_veolia_lodz, WidgetUtility.HEAT, "VeoliaLodzWidgetProvider", "Veolia", "Łódź"),
        provider("gpec", R.string.provider_gpec, WidgetUtility.HEAT, "GpecWidgetProvider", "GPEC", "Gdańsk"),
        provider("kpec_bydgoszcz", R.string.provider_kpec_bydgoszcz, WidgetUtility.HEAT, "KpecBydgoszczWidgetProvider", "KPEC", "Bydgoszcz"),
        provider("mpec_kielce", R.string.provider_mpec_kielce, WidgetUtility.HEAT, "MpecKielceWidgetProvider", "MPEC", "Kielce"),
        provider("mec_koszalin", R.string.provider_mec_koszalin, WidgetUtility.HEAT, "MecKoszalinWidgetProvider", "MEC", "Koszalin"),
        provider("mpec_rzeszow", R.string.provider_mpec_rzeszow, WidgetUtility.HEAT, "MpecRzeszowWidgetProvider", "MPEC", "Rzeszów"),
        provider("opec_gdynia", R.string.provider_opec_gdynia, WidgetUtility.HEAT, "OpecGdyniaWidgetProvider", "OPEC", "Gdynia"),
        provider("pec_gliwice", R.string.provider_pec_gliwice, WidgetUtility.HEAT, "PecGliwiceWidgetProvider", "PEC", "Gliwice"),
        provider("pge_torun_heat", R.string.provider_pge_torun_heat, WidgetUtility.HEAT, "PgeTorunHeatWidgetProvider", "PGE Ciepło", "Toruń"),
        provider("epec_elblag", R.string.provider_epec_elblag, WidgetUtility.HEAT, "EpecElblagWidgetProvider", "EPEC", "Elbląg"),
        provider("sec", R.string.provider_sec, WidgetUtility.HEAT, "SecWidgetProvider", "SEC", "Szczecin"),
        provider("lpec", R.string.provider_lpec, WidgetUtility.HEAT, "LpecWidgetProvider", "LPEC", "Lublin"),
        provider("mpwik_wroclaw", R.string.provider_mpwik_wroclaw, WidgetUtility.WATER, "MpwikWidgetProvider", "MPWiK", "Wrocław"),
        provider("mpwik_warszawa", R.string.provider_mpwik_warszawa, WidgetUtility.WATER, "MpwikWarszawaWidgetProvider", "MPWiK", "Warszawa"),
        provider("wmk", R.string.provider_wmk, WidgetUtility.WATER, "WmkWidgetProvider", "Wodociągi Miasta Krakowa", "Kraków"),
        provider("wm_radom", R.string.provider_wm_radom, WidgetUtility.WATER, "WmRadomWidgetProvider", "Wodociągi", "Radom"),
        provider("aquanet", R.string.provider_aquanet, WidgetUtility.WATER, "AquanetWidgetProvider", "Aquanet", "Poznań"),
        provider("aqua_bielsko", R.string.provider_aqua_bielsko, WidgetUtility.WATER, "AquaBielskoWidgetProvider", "AQUA", "Bielsko-Biała"),
        provider("katowickie_wodociagi", R.string.provider_katowickie_wodociagi, WidgetUtility.WATER, "KatowickieWodociagiWidgetProvider", "KW", "Katowice"),
        provider("zwik_lodz", R.string.provider_zwik_lodz, WidgetUtility.WATER, "ZwikLodzWidgetProvider", "ZWiK", "Łódź"),
        provider("zwik_zielona_gora", R.string.provider_zwik_zielona_gora, WidgetUtility.WATER, "ZwikZielonaGoraWidgetProvider", "ZWiK", "Zielona Góra"),
        provider("pwik_kalisz", R.string.provider_pwik_kalisz, WidgetUtility.WATER, "PwikKaliszWidgetProvider", "PWiK", "Kalisz"),
        provider("wodociagi_plockie", R.string.provider_wodociagi_plockie, WidgetUtility.WATER, "WodociagiPlockieWidgetProvider", "Wodociągi", "Płock"),
        provider("wodociagi_bialostockie", R.string.provider_wodociagi_bialostockie, WidgetUtility.WATER, "WodociagiBialostockieWidgetProvider", "WOBI", "Białystok"),
        provider("wodociagi_kieleckie", R.string.provider_wodociagi_kieleckie, WidgetUtility.WATER, "WodociagiKieleckieWidgetProvider", "Wodociągi", "Kielce"),
        provider("torunskie_wodociagi", R.string.provider_torunskie_wodociagi, WidgetUtility.WATER, "TorunskieWodociagiWidgetProvider", "Toruńskie Wodociągi", "Toruń"),
        provider("pwik_czestochowa", R.string.provider_pwik_czestochowa, WidgetUtility.WATER, "PwikCzestochowaWidgetProvider", "PWiK", "Częstochowa"),
        provider("pwik_olsztyn", R.string.provider_pwik_olsztyn, WidgetUtility.WATER, "PwikOlsztynWidgetProvider", "PWiK", "Olsztyn"),
        provider("pwik_gorzow", R.string.provider_pwik_gorzow, WidgetUtility.WATER, "PwikGorzowWidgetProvider", "PWiK", "Gorzów Wielkopolski"),
        provider("rpwik_tychy", R.string.provider_rpwik_tychy, WidgetUtility.WATER, "RpwikTychyWidgetProvider", "RPWiK", "Tychy"),
        provider("pwik_gliwice", R.string.provider_pwik_gliwice, WidgetUtility.WATER, "PwikGliwiceWidgetProvider", "PWiK", "Gliwice"),
        provider("wpwik_walbrzych", R.string.provider_wpwik_walbrzych, WidgetUtility.WATER, "WpwikWalbrzychWidgetProvider", "WPWiK", "Wałbrzych"),
        provider("mwik_koszalin", R.string.provider_mwik_koszalin, WidgetUtility.WATER, "MwikKoszalinWidgetProvider", "MWiK", "Koszalin"),
        provider("sosnowieckie_wodociagi", R.string.provider_sosnowieckie_wodociagi, WidgetUtility.WATER, "SosnowieckieWodociagiWidgetProvider", "Wodociągi", "Sosnowiec"),
        provider("tarnowskie_wodociagi", R.string.provider_tarnowskie_wodociagi, WidgetUtility.WATER, "TarnowskieWodociagiWidgetProvider", "Wodociągi", "Tarnów"),
        provider("zabrzanskie_wodociagi", R.string.provider_zabrzanskie_wodociagi, WidgetUtility.WATER, "ZabrzanskieWodociagiWidgetProvider", "ZPWiK", "Zabrze"),
        provider("pwik_ruda_slaska", R.string.provider_pwik_ruda_slaska, WidgetUtility.WATER, "PwikRudaSlaskaWidgetProvider", "PWiK", "Ruda Śląska"),
        provider("pwik_rybnik", R.string.provider_pwik_rybnik, WidgetUtility.WATER, "PwikRybnikWidgetProvider", "PWiK", "Rybnik"),
        provider("wik_opole", R.string.provider_wik_opole, WidgetUtility.WATER, "WikOpoleWidgetProvider", "WiK", "Opole"),
        provider("bytomskie_wodociagi", R.string.provider_bytomskie_wodociagi, WidgetUtility.WATER, "BytomskieWodociagiWidgetProvider", "Wodociągi", "Bytom"),
        provider("chspwik", R.string.provider_chspwik, WidgetUtility.WATER, "ChspwikWidgetProvider", "Chorzowsko-Świętochłowickie Przedsiębiorstwo Wodociągów i Kanalizacji", "ChŚPWiK", "Chorzów", "Świętochłowice", "Chorzów, Świętochłowice"),
        provider("dabrowskie_wodociagi", R.string.provider_dabrowskie_wodociagi, WidgetUtility.WATER, "DabrowskieWodociagiWidgetProvider", "Wodociągi", "Dąbrowa Górnicza"),
        provider("gdanskie_wodociagi", R.string.provider_gdanskie_wodociagi, WidgetUtility.WATER, "GdanskieWodociagiWidgetProvider", "Wodociągi", "Gdańsk"),
        provider("puk_rokietnica", R.string.provider_puk_rokietnica, WidgetUtility.WATER, "PukRokietnicaWidgetProvider", "PUK", "Rokietnica"),
        provider("mpwik_lublin", R.string.provider_mpwik_lublin, WidgetUtility.WATER, "MpwikLublinWidgetProvider", "MPWiK", "Lublin"),
        provider("mpwik_rzeszow", R.string.provider_mpwik_rzeszow, WidgetUtility.WATER, "MpwikRzeszowWidgetProvider", "MPWiK", "Rzeszów"),
        provider("mwik_bydgoszcz", R.string.provider_mwik_bydgoszcz, WidgetUtility.WATER, "MwikBydgoszczWidgetProvider", "MWiK", "Bydgoszcz"),
        provider("pewik_gdynia", R.string.provider_pewik_gdynia, WidgetUtility.WATER, "PewikGdyniaWidgetProvider", "PEWIK", "Gdynia"),
        provider("lpwik_legnica", R.string.provider_lpwik_legnica, WidgetUtility.WATER, "LpwikLegnicaWidgetProvider", "LPWiK", "Legnica"),
        provider("psg", R.string.provider_psg, WidgetUtility.GAS, "PsgWidgetProvider", "Polska Spółka Gazownictwa", "Polska")
    )

    val bySourceId: Map<String, SingleProviderEntry> = entries.associateBy { it.sourceId }

    fun find(sourceId: String?): SingleProviderEntry? = sourceId?.let(bySourceId::get)

    fun forLegacyComponent(className: String?): SingleProviderEntry? {
        if (className.isNullOrBlank()) return null
        val simpleName = className.substringAfterLast('.')
        return entries.firstOrNull { it.legacyComponentClass == simpleName }
    }

    /** Inspect the installed manifest registrations so receiver/catalog drift is observable. */
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

    /** Search both sections together; names, aliases, locations and stable IDs are all indexed. */
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

    private fun provider(
        id: String,
        nameRes: Int,
        utility: WidgetUtility,
        component: String,
        vararg searchable: String
    ) = SingleProviderEntry(
        sourceId = id,
        nameRes = nameRes,
        utility = utility,
        legacyComponentClass = component,
        aliases = searchable.toList(),
        location = searchable.lastOrNull().orEmpty()
    )
}
