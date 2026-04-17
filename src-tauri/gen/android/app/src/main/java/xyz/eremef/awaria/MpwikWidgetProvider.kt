package xyz.eremef.awaria

import android.content.Context

class MpwikWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_MPWIK"
    override val primaryColorRes: Int = R.color.brand_mpwik
    override val iconResId: Int = R.drawable.ic_water
    override val labelKey: String = "outages"
    override val sourceKey: String = "water"

    override suspend fun fetchCount(context: Context, settings: List<WidgetSettings>): Int {
        return MpwikProvider().fetchCount(context, settings)
    }
}
