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

        // Update MPWiK Warszawa widgets
        val mpwikWarszawaName = ComponentName(context, MpwikWarszawaWidgetProvider::class.java)
        val mpwikWarszawaIds = appWidgetManager.getAppWidgetIds(mpwikWarszawaName)
        val mpwikWarszawaProvider = MpwikWarszawaWidgetProvider()
        for (id in mpwikWarszawaIds) {
            mpwikWarszawaProvider.updateWidget(context, appWidgetManager, id)
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

        // Update Katowickie Wodociagi widgets
        val katowickieWodociagiName = ComponentName(context, KatowickieWodociagiWidgetProvider::class.java)
        val katowickieWodociagiIds = appWidgetManager.getAppWidgetIds(katowickieWodociagiName)
        val katowickieWodociagiProvider = KatowickieWodociagiWidgetProvider()
        for (id in katowickieWodociagiIds) {
            katowickieWodociagiProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Veolia widgets
        val veoliaName = ComponentName(context, VeoliaWidgetProvider::class.java)
        val veoliaIds = appWidgetManager.getAppWidgetIds(veoliaName)
        val veoliaProvider = VeoliaWidgetProvider()
        for (id in veoliaIds) {
            veoliaProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Veolia Poznan widgets
        val veoliaPoznanName = ComponentName(context, VeoliaPoznanWidgetProvider::class.java)
        val veoliaPoznanIds = appWidgetManager.getAppWidgetIds(veoliaPoznanName)
        val veoliaPoznanProvider = VeoliaPoznanWidgetProvider()
        for (id in veoliaPoznanIds) {
            veoliaPoznanProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Veolia Lodz widgets
        val veoliaLodzName = ComponentName(context, VeoliaLodzWidgetProvider::class.java)
        val veoliaLodzIds = appWidgetManager.getAppWidgetIds(veoliaLodzName)
        val veoliaLodzProvider = VeoliaLodzWidgetProvider()
        for (id in veoliaLodzIds) {
            veoliaLodzProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update ZWIK Lodz widgets
        val zwikLodzName = ComponentName(context, ZwikLodzWidgetProvider::class.java)
        val zwikLodzIds = appWidgetManager.getAppWidgetIds(zwikLodzName)
        val zwikLodzProvider = ZwikLodzWidgetProvider()
        for (id in zwikLodzIds) {
            zwikLodzProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update PWiK Kalisz widgets
        val pwikKaliszName = ComponentName(context, PwikKaliszWidgetProvider::class.java)
        val pwikKaliszIds = appWidgetManager.getAppWidgetIds(pwikKaliszName)
        val pwikKaliszProvider = PwikKaliszWidgetProvider()
        for (id in pwikKaliszIds) {
            pwikKaliszProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update Wodociągi Płockie widgets
        val wodociagiPlockieName = ComponentName(context, WodociagiPlockieWidgetProvider::class.java)
        val wodociagiPlockieIds = appWidgetManager.getAppWidgetIds(wodociagiPlockieName)
        val wodociagiPlockieProvider = WodociagiPlockieWidgetProvider()
        for (id in wodociagiPlockieIds) {
            wodociagiPlockieProvider.updateWidget(context, appWidgetManager, id)
        }

        // Update PWiK Częstochowa widgets
        val pwikCzestochowaName = ComponentName(context, PwikCzestochowaWidgetProvider::class.java)
        val pwikCzestochowaIds = appWidgetManager.getAppWidgetIds(pwikCzestochowaName)
        val pwikCzestochowaProvider = PwikCzestochowaWidgetProvider()
        for (id in pwikCzestochowaIds) {
            pwikCzestochowaProvider.updateWidget(context, appWidgetManager, id)
        }


        return androidx.work.ListenableWorker.Result.success()
    }
}
