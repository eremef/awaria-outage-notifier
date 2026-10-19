package xyz.eremef.awaria

class WodociagiKieleckieWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_WODOCIAGI_KIELECKIE"
    override val primaryColorRes: Int = R.color.utility_water
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "wodociagi_kieleckie"
}
