package xyz.eremef.awaria

import android.appwidget.AppWidgetManager
import android.content.Context
import android.widget.RemoteViews

class MpwikLublinWidgetProvider : BaseWidgetProvider() {
    override val providerId: String = "mpwik_lublin"

    override fun updateAppWidget(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetId: Int,
        useCacheOnly: Boolean
    ) {
        val views = RemoteViews(context.packageName, R.layout.widget_layout)
        
        setupWidgetUI(context, views, appWidgetId, useCacheOnly)
        
        appWidgetManager.updateAppWidget(appWidgetId, views)
    }
}
