package xyz.eremef.awaria

import android.content.Context

class EneaWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_ENEA"
    override val lightPrimary: String = "#00225F" // Enea Blue
    override val darkPrimary: String = "#60a5fa"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "enea"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return EneaProvider().fetchCount(context, settings)
    }
}
