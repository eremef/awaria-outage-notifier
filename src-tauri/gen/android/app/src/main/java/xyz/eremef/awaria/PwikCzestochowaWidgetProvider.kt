package xyz.eremef.awaria

class PwikCzestochowaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PWIK_CZESTOCHOWA"
    override val primaryColorRes: Int = R.color.brand_pwik_czestochowa
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "pwik_czestochowa"
}
