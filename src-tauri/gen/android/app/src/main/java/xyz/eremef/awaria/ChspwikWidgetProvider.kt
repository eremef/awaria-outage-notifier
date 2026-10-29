package xyz.eremef.awaria

/** Legacy source-bound receiver retained for widget catalog and manifest parity. */
class ChspwikWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_CHSPWIK"
    override val primaryColorRes: Int = R.color.utility_water
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "chspwik"
}
