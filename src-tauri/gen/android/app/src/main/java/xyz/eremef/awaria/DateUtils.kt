package xyz.eremef.awaria

import android.util.Log
import java.text.SimpleDateFormat
import java.util.*

object DateUtils {
    private const val TAG = "AwariaDateUtils"

    private fun createFormats() = listOf(
        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") },
        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS", Locale.US),
        SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.US),
        SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US),
        SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US),
        SimpleDateFormat("dd-MM-yyyy HH:mm", Locale.US),
        SimpleDateFormat("yyyy-MM-dd", Locale.US)
    )

    fun parseDate(dateStr: String?): Date? {
        if (dateStr.isNullOrEmpty()) return null
        
        // Clean up the string a bit (some providers might have weird padding)
        val cleanStr = dateStr.trim()
        
        for (format in createFormats()) {
            try {
                val date = format.parse(cleanStr)
                if (date != null) return date
            } catch (e: Exception) {
                // Continue to next format
            }
        }
        Log.w(TAG, "Failed to parse date string: '$dateStr'")
        return null
    }

    /**
     * Returns true if the outage is still active (end date is in the future).
     * If the end date is missing or can't be parsed, returns true by default.
     */
    fun isOutageActive(endDateStr: String?): Boolean {
        if (endDateStr.isNullOrEmpty()) return true
        val end = parseDate(endDateStr) ?: return true
        val now = Date()
        return !end.before(now)
    }
}
