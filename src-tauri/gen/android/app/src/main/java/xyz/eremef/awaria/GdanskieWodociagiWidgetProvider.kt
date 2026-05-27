package xyz.eremef.awaria

class GdanskieWodociagiWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_GDANSKIE_WODOCIAGI"
    override val primaryColorRes: Int = R.color.brand_gdanskie_wodociagi
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "gdanskie_wodociagi"
}
