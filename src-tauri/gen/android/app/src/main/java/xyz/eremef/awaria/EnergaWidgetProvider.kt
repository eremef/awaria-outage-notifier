package xyz.eremef.awaria

class EnergaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ENERGA"
    override val primaryColorRes: Int = R.color.utility_power
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "energa"
}
