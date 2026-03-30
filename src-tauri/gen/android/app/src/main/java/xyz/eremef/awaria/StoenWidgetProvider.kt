package xyz.eremef.awaria

import xyz.eremef.awaria.R
import android.content.Context

class StoenWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_STOEN"
    override val lightPrimary: String = "#c026d3" // Stoen Magenta
    override val darkPrimary: String = "#e879f9"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchStoenAlertCount(settings)
    }
}
