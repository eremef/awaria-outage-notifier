package xyz.eremef.awaria

class MpwikWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_MPWIK"
    override val lightPrimary: String = "#0077D9" // Water Blue
    override val darkPrimary: String = "#4DA6FF"
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "water"

    override suspend fun fetchCount(settings: List<WidgetSettings>): Int {
        return fetchMpwikAlertCount(settings)
    }
}
