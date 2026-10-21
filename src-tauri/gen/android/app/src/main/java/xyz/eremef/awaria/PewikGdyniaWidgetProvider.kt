package xyz.eremef.awaria

class PewikGdyniaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PEWIK_GDYNIA"
    override val primaryColorRes: Int = R.color.utility_water
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "pewik_gdynia"
}
