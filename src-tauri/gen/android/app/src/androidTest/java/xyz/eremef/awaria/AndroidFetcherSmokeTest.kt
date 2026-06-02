package xyz.eremef.awaria

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

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
        
        assertNotNull("HTML response from PWiK Kalisz should not be null", html)
        assertTrue("HTML should contain standard article indicators", html!!.contains("ArticleID"))
    }

    @Test
    fun testPsgWebViewFetcher_LiveScraping() {
        val appContext = InstrumentationRegistry.getInstrumentation().targetContext
        val latch = CountDownLatch(1)
        var resultHtml: String? = null

        // WebView requires UI Thread interaction
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            try {
                // Execute the native webview scraper directly
                resultHtml = PsgWebViewFetcher.fetchHtmlNative(appContext)
                latch.countDown()
            } catch (e: Exception) {
                latch.countDown()
            }
        }

        latch.await(30, TimeUnit.SECONDS)
        assertNotNull("PSG WebView Scraper returned null response", resultHtml)
        assertTrue("PSG HTML should contain standard outage keywords", resultHtml!!.contains("Wykaz wyłączeń") || resultHtml!!.contains("Przerwy"))
    }

    @Test
    fun testGpecWebViewFetcher_LiveScraping() {
        val appContext = InstrumentationRegistry.getInstrumentation().targetContext
        val latch = CountDownLatch(1)
        var resultHtml: String? = null

        // WebView requires UI Thread interaction
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            try {
                // Execute the native webview scraper directly
                resultHtml = GpecWebViewFetcher.fetchHtmlNative(appContext)
                latch.countDown()
            } catch (e: Exception) {
                latch.countDown()
            }
        }

        latch.await(30, TimeUnit.SECONDS)
        assertNotNull("GPEC WebView Scraper returned null response", resultHtml)
        assertTrue("GPEC HTML should contain standard page structure", resultHtml!!.lowercase().contains("gpec") || resultHtml!!.contains("<html"))
    }
}
