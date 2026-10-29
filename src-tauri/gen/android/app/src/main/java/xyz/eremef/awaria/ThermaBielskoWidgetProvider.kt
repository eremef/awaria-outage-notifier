package xyz.eremef.awaria

/** Legacy source-bound receiver retained for widget catalog and manifest parity. */
class ThermaBielskoWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_THERMA_BIELSKO"
    override val primaryColorRes: Int = R.color.utility_heat
    override val iconResId: Int = R.drawable.ic_heating
    override val labelKey: String = "outages"
    override val sourceKey: String = "therma_bielsko"
}
