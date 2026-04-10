package xyz.eremef.awaria

import android.content.Context

class StoenWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_STOEN"
    override val lightPrimary: String = "#ea1b0a"
    override val darkPrimary: String = "#f87171"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "stoen"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return StoenProvider().fetchCount(context, settings)
    }
}
