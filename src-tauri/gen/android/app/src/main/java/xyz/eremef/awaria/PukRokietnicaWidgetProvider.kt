package xyz.eremef.awaria

import android.content.Context

class PukRokietnicaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PUK_ROKIETNICA"
    override val primaryColorRes: Int = R.color.brand_puk_rokietnica
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "puk_rokietnica"
}
