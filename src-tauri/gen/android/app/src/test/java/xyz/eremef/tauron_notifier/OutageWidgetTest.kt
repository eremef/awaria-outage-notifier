package xyz.eremef.tauron_notifier

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class OutageWidgetTest {

    private val provider = OutageWidgetProvider()

    @Test
    fun testParseSettings() {
        val json = """
            {
                "cityGAID": 123,
                "streetGAID": 456,
                "houseNo": "10A",
                "streetName": "Rozbrat",
                "theme": "dark",
                "language": "pl"
            }
        """.trimIndent()

        val settings = provider.parseSettings(json)
        assertEquals(123L, settings?.cityGAID)
        assertEquals(456L, settings?.streetGAID)
        assertEquals("10A", settings?.houseNo)
        assertEquals("Rozbrat", settings?.streetName)
        assertEquals("dark", settings?.theme)
        assertEquals("pl", settings?.language)
    }

    @Test
    fun testParseSettingsCorrupt() {
        val json = "{ invalid json }"
        val settings = provider.parseSettings(json)
        assertNull(settings)
    }

    @Test
    fun testParseOutageItems() {
        val json = """
            {
                "OutageItems": [
                    { "Message": "Outage at Rozbrat 12, Wrocław" },
                    { "Message": "Maintenance at Legnicka 5, Wrocław" },
                    { "Message": "Broken pipe at Rozbrat 1, Wrocław" }
                ]
            }
        """.trimIndent()

        val count = provider.parseOutageItems(json, "Rozbrat")
        assertEquals(2, count)
    }

    @Test
    fun testParseOutageItemsPartialMatch() {
        val json = """
            {
                "OutageItems": [
                    { "Message": "Awaria na Probusa 5" },
                    { "Message": "Prace na Jana Pawła II" },
                    { "Message": "Utrudnienia na Pawła" }
                ]
            }
        """.trimIndent()

        // "Henryka Probusa" -> matches "Probusa"
        var count = provider.parseOutageItems(json, "Henryka Probusa")
        assertEquals(1, count)

        // "Jana Pawła II" -> matches "Pawła" and "Jana Pawła"
        count = provider.parseOutageItems(json, "Jana Pawła II")
        assertEquals(2, count)
    }

    @Test
    fun testParseOutageItemsNoMatch() {
        val json = """
            {
                "OutageItems": [
                    { "Message": "Maintenance work in progress" },
                    { "Message": "Other outage" }
                ]
            }
        """.trimIndent()

        // "Main" matching "Maintenance" should fail due to word boundaries
        val count = provider.parseOutageItems(json, "Main St")
        assertEquals(0, count)
        
        val count2 = provider.parseOutageItems(json, "Rozbrat")
        assertEquals(0, count2)
    }

    @Test
    fun testParseOutageItemsWithDates() {
        val now = java.util.Date()
        val format = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", java.util.Locale.US)
        format.timeZone = java.util.TimeZone.getTimeZone("UTC")
        
        val pastDate = format.format(java.util.Date(now.time - 3600000)) // 1 hour ago
        val futureDate = format.format(java.util.Date(now.time + 3600000)) // 1 hour from now

        val json = """
            {
                "OutageItems": [
                    { "Message": "Past Outage at Rozbrat 1", "EndDate": "${pastDate}" },
                    { "Message": "Future Outage at Rozbrat 2", "EndDate": "${futureDate}" },
                    { "Message": "Outage No Date at Rozbrat 3" }
                ]
            }
        """.trimIndent()

        val count = provider.parseOutageItems(json, "Rozbrat")
        assertEquals(2, count) // Future and No Date should be counted
    }
}
