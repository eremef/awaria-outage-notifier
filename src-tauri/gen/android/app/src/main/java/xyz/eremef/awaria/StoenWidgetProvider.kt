package xyz.eremef.awaria

import xyz.eremef.awaria.R
import android.content.Context

class StoenWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_STOEN"
    override val lightPrimary: String = "#ea1b0a"
    override val darkPrimary: String = "#f87171"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchStoenAlertCount(settings)
    }
}
