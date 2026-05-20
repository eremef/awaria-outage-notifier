package xyz.eremef.awaria

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters

class WidgetUpdateWorker(
    private val context: Context,
    workerParams: WorkerParameters
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): androidx.work.ListenableWorker.Result {
        // CRITICAL: Initialize TLS verifier for Rust network requests.
        WidgetUtils.initVerifier(context)
        
        val appWidgetManager = AppWidgetManager.getInstance(context)
        
        // Update Tauron widgets
        val tauronName = ComponentName(context, TauronWidgetProvider::class.java)
        val tauronIds = appWidgetManager.getAppWidgetIds(tauronName)
        val tauronProvider = TauronWidgetProvider()
        Log.d("WidgetWorker", "Updating ${tauronIds.size} Tauron widgets")
        for (id in tauronIds) {
            try {
                tauronProvider.updateWidget(context, appWidgetManager, id)
            } catch (e: Exception) {
                Log.e("WidgetWorker", "Failed to update Tauron widget $id", e)
            }
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

        // Update PGE widgets
        val pgeName = ComponentName(context, PgeWidgetProvider::class.java)
        val pgeIds = appWidgetManager.getAppWidgetIds(pgeName)
        val pgeProvider = PgeWidgetProvider()
        for (id in pgeIds) {
            pgeProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Stoen widgets
        val stoenName = ComponentName(context, StoenWidgetProvider::class.java)
        val stoenIds = appWidgetManager.getAppWidgetIds(stoenName)
        val stoenProvider = StoenWidgetProvider()
        for (id in stoenIds) {
            stoenProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Tri-Status widgets
        val triName = ComponentName(context, TriWidgetProvider::class.java)
        val triIds = appWidgetManager.getAppWidgetIds(triName)
        val triProvider = TriWidgetProvider()
        Log.d("WidgetWorker", "Updating ${triIds.size} Tri-Status widgets")
        for (id in triIds) {
            try {
                triProvider.updateWidget(context, appWidgetManager, id)
            } catch (e: Exception) {
                Log.e("WidgetWorker", "Failed to update Tri-Status widget $id", e)
            }
        }

        // Update All-Status widgets
        val allName = ComponentName(context, AllWidgetProvider::class.java)
        val allIds = appWidgetManager.getAppWidgetIds(allName)
        val allProvider = AllWidgetProvider()
        Log.d("WidgetWorker", "Updating ${allIds.size} All-Status widgets")
        for (id in allIds) {
            try {
                allProvider.updateWidget(context, appWidgetManager, id)
            } catch (e: Exception) {
                Log.e("WidgetWorker", "Failed to update All-Status widget $id", e)
            }
        }

        // Update PSG widgets
        val psgName = ComponentName(context, PsgWidgetProvider::class.java)
        val psgIds = appWidgetManager.getAppWidgetIds(psgName)
        val psgProvider = PsgWidgetProvider()
        for (id in psgIds) {
            psgProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update WMK widgets
        val wmkName = ComponentName(context, WmkWidgetProvider::class.java)
        val wmkIds = appWidgetManager.getAppWidgetIds(wmkName)
        val wmkProvider = WmkWidgetProvider()
        for (id in wmkIds) {
            wmkProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Aquanet widgets
        val aquanetName = ComponentName(context, AquanetWidgetProvider::class.java)
        val aquanetIds = appWidgetManager.getAppWidgetIds(aquanetName)
        val aquanetProvider = AquanetWidgetProvider()
        for (id in aquanetIds) {
            aquanetProvider.updateWidget(context, appWidgetManager, id)
        }

        return androidx.work.ListenableWorker.Result.success()
    }
}
