package xyz.eremef.awaria

import android.content.Context

class GpecWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_GPEC"
    override val primaryColorRes: Int = R.color.brand_gpec
    override val iconResId: Int = R.drawable.ic_heating
    override val labelKey: String = "outages"
    override val sourceKey: String = "gpec"

}
