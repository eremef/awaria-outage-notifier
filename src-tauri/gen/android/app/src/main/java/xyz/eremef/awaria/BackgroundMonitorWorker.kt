package xyz.eremef.awaria

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import org.json.JSONObject
import java.io.File

class BackgroundMonitorWorker(
    private val context: Context,
    workerParams: WorkerParameters
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): androidx.work.ListenableWorker.Result {
        try {
            android.util.Log.i("AwariaBgMonitor", "Background worker execution started")
            val settingsFile = findSettingsFile(context) ?: run {
                android.util.Log.w("AwariaBgMonitor", "Settings file not found, skipping background fetch")
                return androidx.work.ListenableWorker.Result.success()
            }
            val jsonString = settingsFile.readText(Charsets.UTF_8)
            android.util.Log.d("AwariaBgMonitor", "Settings loaded (length: ${jsonString.length}), calling Rust...")
            
            // Trigger the full fetch and notify logic in Rust
            WidgetUtils.fetchAndNotifyFromRust(context, jsonString)
            
            android.util.Log.i("AwariaBgMonitor", "Rust background fetch/notify completed successfully")
            return androidx.work.ListenableWorker.Result.success()
        } catch (e: Exception) {
            android.util.Log.e("AwariaBgMonitor", "CRITICAL ERROR in background monitoring", e)
            return androidx.work.ListenableWorker.Result.retry()
        }
    }

    private fun findSettingsFile(context: Context): File? {
        val candidates = mutableListOf<File>()
        candidates.add(File(context.filesDir, "settings.json"))
        candidates.add(File(context.dataDir, "settings.json"))
        context.filesDir.parentFile?.let { parent ->
            candidates.add(File(parent, "app_data/settings.json"))
        }
        return candidates.firstOrNull { it.exists() && it.canRead() }
    }
}
