package xyz.eremef.awaria

import xyz.eremef.awaria.R

import android.content.Context

class TauronWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_TAURON"
    override val lightPrimary: String = "#D9006C" // Original Magenta
    override val darkPrimary: String = "#FF4DA6"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchTauronAlertCount(settings)
    }
}
