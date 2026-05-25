package xyz.eremef.awaria

class ZwikLodzWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ZWIK_LODZ"
    override val primaryColorRes: Int = R.color.brand_zwik_lodz
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "zwik_lodz"
}
