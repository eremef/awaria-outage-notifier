package xyz.eremef.awaria

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters

class WidgetUpdateWorker(
    private val context: Context,
    workerParams: WorkerParameters
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): androidx.work.ListenableWorker.Result {
        val appWidgetManager = AppWidgetManager.getInstance(context)
        
        // Update Tauron widgets
        val tauronName = ComponentName(context, TauronWidgetProvider::class.java)
        val tauronIds = appWidgetManager.getAppWidgetIds(tauronName)
        val tauronProvider = TauronWidgetProvider()
        for (id in tauronIds) {
            tauronProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update MPWiK widgets
        val mpwikName = ComponentName(context, MpwikWidgetProvider::class.java)
        val mpwikIds = appWidgetManager.getAppWidgetIds(mpwikName)
        val mpwikProvider = MpwikWidgetProvider()
        for (id in mpwikIds) {
            mpwikProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Energa widgets
        val energaName = ComponentName(context, EnergaWidgetProvider::class.java)
        val energaIds = appWidgetManager.getAppWidgetIds(energaName)
        val energaProvider = EnergaWidgetProvider()
        for (id in energaIds) {
            energaProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Fortum widgets
        val fortumName = ComponentName(context, FortumWidgetProvider::class.java)
        val fortumIds = appWidgetManager.getAppWidgetIds(fortumName)
        val fortumProvider = FortumWidgetProvider()
        for (id in fortumIds) {
            fortumProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Enea widgets
        val eneaName = ComponentName(context, EneaWidgetProvider::class.java)
        val eneaIds = appWidgetManager.getAppWidgetIds(eneaName)
        val eneaProvider = EneaWidgetProvider()
        for (id in eneaIds) {
            eneaProvider.updateWidget(context, appWidgetManager, id)
        }

        return androidx.work.ListenableWorker.Result.success()
    }
}
