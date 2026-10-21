package xyz.eremef.awaria

class OpecGdyniaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_OPEC_GDYNIA"
    override val primaryColorRes: Int = R.color.utility_heat
    override val iconResId: Int = R.drawable.ic_heating
    override val labelKey: String = "outages"
    override val sourceKey: String = "opec_gdynia"
}
