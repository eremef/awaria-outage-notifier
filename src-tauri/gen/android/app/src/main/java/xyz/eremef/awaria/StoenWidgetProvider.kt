package xyz.eremef.awaria

import android.content.Context

class StoenWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_STOEN"
    override val primaryColorRes: Int = R.color.brand_stoen
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "stoen"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return StoenProvider().fetchCount(context, settings)
    }
}
