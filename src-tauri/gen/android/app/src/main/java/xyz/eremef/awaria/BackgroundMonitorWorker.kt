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
            val settingsFile = findSettingsFile(context) ?: return androidx.work.ListenableWorker.Result.success()
            val jsonString = settingsFile.readText(Charsets.UTF_8)
            
            // Trigger the full fetch and notify logic in Rust
            WidgetUtils.fetchAndNotifyFromRust(context, jsonString)
            
            return androidx.work.ListenableWorker.Result.success()
        } catch (e: Exception) {
            android.util.Log.e("AwariaBgMonitor", "Error in background monitoring", e)
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
