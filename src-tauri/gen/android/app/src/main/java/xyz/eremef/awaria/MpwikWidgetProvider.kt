package xyz.eremef.awaria

import xyz.eremef.awaria.R

import android.content.Context

class MpwikWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_MPWIK"
    override val lightPrimary: String = "#0077D9" // Water Blue
    override val darkPrimary: String = "#4DA6FF"
    override val iconResId: Int = R.drawable.ic_water

    override suspend fun fetchCount(settingsList: List<WidgetSettings>): Int {
        return fetchMpwikAlertCount(settingsList)
    }
}
