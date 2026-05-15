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
            
            // CRITICAL: Initialize TLS verifier for Rust network requests.
            // Without this, background fetches will fail with SSL errors if the app process was freshly started.
            WidgetUtils.initVerifier(context)
            android.util.Log.d("AwariaBgMonitor", "TLS Verifier initialized")

            val settingsFile = findSettingsFile(context) ?: run {
                android.util.Log.w("AwariaBgMonitor", "Settings file not found, skipping background fetch")
                return androidx.work.ListenableWorker.Result.success()
            }
            val jsonString = settingsFile.readText(Charsets.UTF_8)
            android.util.Log.d("AwariaBgMonitor", "Settings loaded (length: ${jsonString.length}), calling Rust fetchAndNotifyFromRust...")
            
            // Trigger the full fetch and notify logic in Rust
            WidgetUtils.fetchAndNotifyFromRust(context, jsonString)
            
            android.util.Log.i("AwariaBgMonitor", "Rust background fetch/notify call finished.")
            return androidx.work.ListenableWorker.Result.success()
        } catch (e: Exception) {
            android.util.Log.e("AwariaBgMonitor", "CRITICAL ERROR in background monitoring: ${e.message}", e)
            return androidx.work.ListenableWorker.Result.retry()
        }
    }

    private fun findSettingsFile(context: Context): File? {
        val candidates = mutableListOf<File>()
        candidates.add(File(context.filesDir, "settings.json"))
        candidates.add(File(context.dataDir, "settings.json"))
        candidates.add(File(context.filesDir, "xyz.eremef.awaria/settings.json"))
        candidates.add(File(context.filesDir, "Awaria/settings.json"))
        context.filesDir.parentFile?.let { parent ->
            candidates.add(File(parent, "app_data/settings.json"))
        }
        return candidates.firstOrNull { it.exists() && it.canRead() }
    }
}
