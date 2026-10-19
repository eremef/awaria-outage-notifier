package xyz.eremef.awaria

class EnergaCieploKaliskieWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ENERGA_CIEPLO_KALISKIE"
    override val primaryColorRes: Int = R.color.utility_heat
    override val iconResId: Int = R.drawable.ic_heating
    override val labelKey: String = "outages"
    override val sourceKey: String = "energa_cieplo_kaliskie"
}
