package xyz.eremef.awaria

import android.content.Context

class PsgWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PSG"
    override val primaryColorRes: Int = R.color.brand_psg
    override val iconResId: Int = R.drawable.ic_gas
    override val labelKey: String = "outages"
    override val sourceKey: String = "psg"
}
