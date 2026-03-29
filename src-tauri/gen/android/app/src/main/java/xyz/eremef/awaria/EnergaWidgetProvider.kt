package xyz.eremef.awaria

import xyz.eremef.awaria.R
import android.content.Context

class EnergaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ENERGA"
    override val lightPrimary: String = "#0160a9" // Energa Blue
    override val darkPrimary: String = "#0180ff"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchEnergaAlertCount(settings)
    }
}
