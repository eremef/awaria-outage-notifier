package xyz.eremef.awaria

import android.content.Context
import android.appwidget.AppWidgetManager
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager

class PsgWidgetProvider : BaseWidgetProvider() {
    override val refreshAction: String = "xyz.eremef.awaria.ACTION_REFRESH_PSG"
    override val primaryColorRes: Int = R.color.utility_gas
    override val iconResId: Int = R.drawable.ic_gas
    override val labelKey: String = "outages"
    override val sourceKey: String = "psg"

    override fun onUpdate(
            context: Context,
            appWidgetManager: AppWidgetManager,
            appWidgetIds: IntArray
    ) {
        WidgetUtils.initVerifier(context)
        scheduleWork(context)
        // Show placeholder immediately
        for (appWidgetId in appWidgetIds) {
            showLoadingPlaceholder(context, appWidgetManager, appWidgetId)
        }
        val request = OneTimeWorkRequestBuilder<WidgetUpdateWorker>().build()
        WorkManager.getInstance(context).enqueue(request)
    }
}
