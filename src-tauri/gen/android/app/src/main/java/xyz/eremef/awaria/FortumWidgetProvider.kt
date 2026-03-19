package xyz.eremef.awaria

import xyz.eremef.awaria.R

import android.content.Context

class FortumWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_FORTUM"
    override val lightPrimary: String = "#00A859" // Fortum Green
    override val darkPrimary: String = "#00C86B"
    override val iconResId: Int = R.drawable.ic_electricity

    override suspend fun fetchCount(settingsList: List<WidgetSettings>): Int {
        return fetchFortumAlertCount(settingsList)
    }
}
