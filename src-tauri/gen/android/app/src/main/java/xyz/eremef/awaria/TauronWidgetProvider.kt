package xyz.eremef.awaria

import xyz.eremef.awaria.R

import android.content.Context

class TauronWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_TAURON"
    override val primaryColorRes: Int = R.color.brand_tauron
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "tauron"

}
