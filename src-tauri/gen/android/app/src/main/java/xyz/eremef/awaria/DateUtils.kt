package xyz.eremef.awaria

import android.util.Log
import java.text.SimpleDateFormat
import java.util.*

object DateUtils {
    private const val TAG = "AwariaDateUtils"

    /**
     * Parses a date string.
     * If formatString is provided, it tries it first.
     * If it fails (or is null), it silently tries a list of common fallbacks.
     * Only logs a warning if all attempts fail.
     */
    fun parseDate(dateStr: String?, formatString: String? = null): Date? {
        if (dateStr.isNullOrEmpty()) return null
        val cleanStr = dateStr.trim()

        // 1. Try the suggested format if provided
        if (formatString != null) {
            try {
                val sdf = SimpleDateFormat(formatString, Locale.US)
                sdf.isLenient = true
                if (formatString.endsWith("'Z'")) sdf.timeZone = TimeZone.getTimeZone("UTC")
                val date = sdf.parse(cleanStr)
                if (date != null) return date
            } catch (e: Exception) {
                // Ignore failure, try fallbacks
            }
        }

        // 2. Try fallbacks
        val patterns = listOf(
            "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'",
            "yyyy-MM-dd'T'HH:mm:ss'Z'",
            "yyyy-MM-dd'T'HH:mm:ss.SSS",
            "yyyy-MM-dd'T'HH:mm:ss",
            "yyyy-MM-dd HH:mm:ss",
            "yyyy-MM-dd HH:mm",
            "dd-MM-yyyy HH:mm",
            "yyyy-MM-dd"
        )
        
        for (pattern in patterns) {
            // Skip if it's the same pattern we already tried
            if (pattern == formatString) continue
            
            try {
                val sdf = SimpleDateFormat(pattern, Locale.US)
                if (pattern.endsWith("'Z'")) sdf.timeZone = TimeZone.getTimeZone("UTC")
                val date = sdf.parse(cleanStr)
                if (date != null) return date
            } catch (e: Exception) {
                // Continue
            }
        }
        
        Log.w(TAG, "Failed to parse date string: '$dateStr' (tried hint: $formatString and ${patterns.size} fallbacks)")
        return null
    }

    /**
     * Returns true if the outage is still active (end date is in the future).
     * If the end date is missing or can't be parsed, returns true by default.
     */
    fun isOutageActive(endDateStr: String?, formatString: String? = null): Boolean {
        if (endDateStr.isNullOrEmpty()) return true
        val end = parseDate(endDateStr, formatString) ?: return true
        val now = Date()
        return !end.before(now)
    }
}
