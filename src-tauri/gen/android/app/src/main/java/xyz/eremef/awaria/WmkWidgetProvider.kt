package xyz.eremef.awaria

import android.content.Context

class WmkWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_WMK"
    override val primaryColorRes: Int = R.color.brand_wmk
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "wmk"

}
