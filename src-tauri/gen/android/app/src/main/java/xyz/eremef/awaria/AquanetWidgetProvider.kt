package xyz.eremef.awaria

import android.content.Context

class AquanetWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_AQUANET"
    override val primaryColorRes: Int = R.color.brand_aquanet
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "aquanet"
}
