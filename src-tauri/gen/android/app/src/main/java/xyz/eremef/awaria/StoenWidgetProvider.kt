package xyz.eremef.awaria

class StoenWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_STOEN"
    override val primaryColorRes: Int = R.color.utility_power
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "stoen"
}
