package xyz.eremef.awaria

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Instrumentation test to run live fetches and WebView interactions directly on Android.
 * Bypasses standard unit test environment to test actual Conscrypt TLS layer and native WebView context.
 */
@RunWith(AndroidJUnit4::class)
class AndroidFetcherSmokeTest {

    @Test
    fun testPwikKaliszFetcher_LiveFetch() {
        val url = "https://wodociagi-kalisz.pl/Wy%C5%82%C4%85czenia"
        val html = PwikKaliszFetcher.fetchUrl(url)
        
        if (html == null) {
            System.err.println("[WARN] PWiK Kalisz fetch returned null (might be offline or blocking CI IP range). Skipping assertions.")
            return
        }
        assertTrue("HTML should contain standard article indicators", html.contains("ArticleID"))
    }

    @Test
    fun testPsgWebViewFetcher_LiveScraping() {
        val appContext = InstrumentationRegistry.getInstrumentation().targetContext
        var resultHtml: String? = null

        try {
            // Execute the native webview scraper directly (internally handles Main dispatcher)
            resultHtml = PsgWebViewFetcher.fetchHtmlNative(appContext)
        } catch (e: Exception) {
            e.printStackTrace()
        }

        if (resultHtml == null) {
            System.err.println("[WARN] PSG WebView Scraper returned null (might be offline or blocking CI IP range). Skipping assertions.")
            return
        }
        assertTrue("PSG HTML should contain standard outage keywords", resultHtml!!.contains("Wykaz wyłączeń") || resultHtml!!.contains("Przerwy"))
    }

    @Test
    fun testGpecWebViewFetcher_LiveScraping() {
        val appContext = InstrumentationRegistry.getInstrumentation().targetContext
        var resultHtml: String? = null

        try {
            // Execute the native webview scraper directly (internally handles Main dispatcher)
            resultHtml = GpecWebViewFetcher.fetchHtmlNative(appContext)
        } catch (e: Exception) {
            e.printStackTrace()
        }

        if (resultHtml == null) {
            System.err.println("[WARN] GPEC WebView Scraper returned null (might be offline or blocking CI IP range). Skipping assertions.")
            return
        }
        assertTrue("GPEC HTML should contain standard page structure", resultHtml!!.lowercase().contains("gpec") || resultHtml!!.contains("<html") || resultHtml == "Brak przerw")
    }
}
