package xyz.eremef.awaria

import xyz.eremef.awaria.R
import android.content.Context

class PgeWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PGE"
    override val lightPrimary: String = "#1E3A8A" // PGE Blue
    override val darkPrimary: String = "#60a5fa"
    override val iconResId: Int = R.drawable.ic_electricity
    override val labelKey: String = "outages"
    override val sourceKey: String = "pge"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchPgeAlertCount(settings)
    }
}
