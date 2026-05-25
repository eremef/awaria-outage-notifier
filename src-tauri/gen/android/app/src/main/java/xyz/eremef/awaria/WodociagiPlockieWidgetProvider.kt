package xyz.eremef.awaria

class WodociagiPlockieWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_WODOCIAGI_PLOCKIE"
    override val primaryColorRes: Int = R.color.brand_wodociagi_plockie
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "wodociagi_plockie"
}
