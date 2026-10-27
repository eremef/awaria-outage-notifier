package xyz.eremef.awaria

class SosnowieckieWodociagiWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_SOSNOWIECKIE_WODOCIAGI"
    override val primaryColorRes: Int = R.color.utility_water
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "sosnowieckie_wodociagi"
}
