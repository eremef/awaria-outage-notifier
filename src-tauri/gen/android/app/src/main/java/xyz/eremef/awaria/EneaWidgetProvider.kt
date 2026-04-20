package xyz.eremef.awaria

import android.content.Context

class EneaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ENEA"
    override val primaryColorRes: Int = R.color.brand_enea
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "enea"

}
