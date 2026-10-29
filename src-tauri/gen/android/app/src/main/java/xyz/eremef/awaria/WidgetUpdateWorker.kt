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

    override suspend fun doWork(): Result {
        WidgetUtils.initVerifier(context)
        val manager = AppWidgetManager.getInstance(context)
        var failedWidgets = 0

        // The generic receiver owns new single-provider widgets; configuration is resolved by ID.
        failedWidgets += updateProvider(manager, SingleWidgetProvider::class.java, SingleWidgetProvider())

        // Compare the explicit catalog with packaged receivers, then update both sets so a catalog
        // omission is logged without stranding a legacy widget placed by an earlier app release.
        SingleProviderCatalog.manifestParityIssues(context).forEach { issue ->
            Log.e(TAG, issue)
        }
        val legacyComponents = SingleProviderCatalog.entries.map { it.legacyComponentClass }
            .toSet() + SingleProviderCatalog.legacyManifestComponents(context)
        legacyComponents.sorted().forEach { className ->
            try {
                val providerClass = Class.forName("${context.packageName}.$className")
                    .asSubclass(BaseWidgetProvider::class.java)
                val provider = providerClass.getDeclaredConstructor().newInstance()
                failedWidgets += updateProvider(manager, providerClass, provider)
            } catch (error: Exception) {
                Log.e(TAG, "Could not update legacy widget component $className", error)
            }
        }

        failedWidgets += updateProvider(manager, TriWidgetProvider::class.java, TriWidgetProvider())
        failedWidgets += updateProvider(manager, AllWidgetProvider::class.java, AllWidgetProvider())
        Log.d(TAG, "Widget update finished; failed widget instances: $failedWidgets")
        // Per-widget failures do not stop updates for other instances or providers.
        return Result.success()
    }

    private suspend fun updateProvider(
        manager: AppWidgetManager,
        providerClass: Class<out BaseWidgetProvider>,
        provider: BaseWidgetProvider
    ): Int {
        val component = ComponentName(context, providerClass)
        val ids = try {
            manager.getAppWidgetIds(component)
        } catch (error: Exception) {
            Log.e(TAG, "Could not enumerate ${providerClass.simpleName}", error)
            return 0
        }
        var failed = 0
        ids.forEach { id ->
            try {
                provider.updateWidget(context, manager, id)
            } catch (error: Exception) {
                failed += 1
                Log.e(TAG, "Failed to update ${providerClass.simpleName} widget $id", error)
            }
        }
        return failed
    }

    companion object {
        private const val TAG = "WidgetWorker"
    }
}
