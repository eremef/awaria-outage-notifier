package xyz.eremef.awaria

import android.content.Context

class PgeWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PGE"
    override val primaryColorRes: Int = R.color.brand_pge
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "pge"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return PgeProvider().fetchCount(context, settings)
    }
}
