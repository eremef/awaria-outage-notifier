package xyz.eremef.awaria

import android.content.Context

class EnergaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ENERGA"
    override val lightPrimary: String = "#0160a9" // Energa Blue
    override val darkPrimary: String = "#0180ff"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "energa"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return EnergaProvider().fetchCount(context, settings)
    }
}
