package xyz.eremef.awaria

import android.util.Log
import androidx.annotation.Keep
import java.net.HttpURLConnection
import java.net.URL

/**
 * Simple HTTP fetcher for PWiK Kalisz using Android's native BoringSSL/Conscrypt TLS stack.
 * Rust's rustls cannot establish TLS with wodociagi-kalisz.pl due to cipher suite incompatibility,
 * so we delegate HTTP fetches to Android's HttpURLConnection which uses the system TLS stack.
 */
@Keep
object PwikKaliszFetcher {
    private const val TAG = "PwikKaliszFetcher"
    private const val USER_AGENT =
        "Mozilla/5.0 (Linux; Android 13; SM-G998B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
    private const val TIMEOUT_MS = 15000

    /**
     * Fetches the given URL and returns the response body as a String.
     * Called from Rust via JNI.
     * Returns null on failure.
     */
    @Keep
    @JvmStatic
    fun fetchUrl(url: String): String? {
        return try {
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("User-Agent", USER_AGENT)
            conn.setRequestProperty("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            conn.setRequestProperty("Accept-Language", "pl-PL,pl;q=0.9,en-US;q=0.8,en;q=0.7")
            conn.connectTimeout = TIMEOUT_MS
            conn.readTimeout = TIMEOUT_MS
            conn.instanceFollowRedirects = true

            val code = conn.responseCode
            if (code in 200..299) {
                val body = conn.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
                conn.disconnect()
                Log.i(TAG, "Fetched $url (${body.length} bytes)")
                body
            } else {
                Log.w(TAG, "HTTP $code for $url")
                conn.disconnect()
                null
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to fetch $url: ${e.message}")
            null
        }
    }
}
