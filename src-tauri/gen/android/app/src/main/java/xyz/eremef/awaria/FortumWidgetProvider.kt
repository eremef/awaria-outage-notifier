package xyz.eremef.awaria

class FortumWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_FORTUM"
    override val lightPrimary: String = "#00A859" // Fortum Green
    override val darkPrimary: String = "#00C86B"
    override val iconResId: Int = R.drawable.ic_heating
    override val labelKey: String = "outages"
    override val sourceKey: String = "fortum"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchFortumAlertCount(settings)
    }
}
