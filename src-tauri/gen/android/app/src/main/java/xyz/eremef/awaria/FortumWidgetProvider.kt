package xyz.eremef.awaria

import android.content.Context

class FortumWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_FORTUM"
    override val primaryColorRes: Int = R.color.utility_heat
    override val iconResId: Int = R.drawable.ic_heating
    override val labelKey: String = "outages"
    override val sourceKey: String = "fortum"
}
