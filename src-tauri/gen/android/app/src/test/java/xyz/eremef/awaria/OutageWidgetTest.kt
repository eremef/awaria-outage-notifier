package xyz.eremef.awaria

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class OutageWidgetTest {

    private val provider = TauronWidgetProvider()

    @Test
    fun testParseOutageItems() {
        val json =
                """
            {
                "OutageItems": [
                    { "Message": "Outage at Kuźnicza 12, Wrocław" },
                    { "Message": "Maintenance at Legnicka 5, Wrocław" },
                    { "Message": "Broken pipe at Kuźnicza 1, Wrocław" }
                ]
            }
        """.trimIndent()

        val count = provider.parseOutageItems(json, "Kuźnicza", null)
        assertEquals(2, count)
    }

    @Test
    fun testParseOutageItemsPartialMatch() {
        val json =
                """
            {
                "OutageItems": [
                    { "Message": "Awaria na Probusa 5" },
                    { "Message": "Prace na Jana Pawła II" },
                    { "Message": "Utrudnienia na Pawła" }
                ]
            }
        """.trimIndent()

        // "Probusa" as streetName1, "Henryka" as streetName2
        // compound "Henryka Probusa" doesn't match, but "Probusa" word matches
        var count = provider.parseOutageItems(json, "Probusa", "Henryka")
        assertEquals(1, count)

        // "Pawła" as streetName1, "Jana" as streetName2
        // compound "Jana Pawła" matches "Jana Pawła II", word "Pawła" matches "Utrudnienia na Pawła"
        count = provider.parseOutageItems(json, "Pawła", "Jana")
        assertEquals(2, count)
    }

    @Test
    fun testParseOutageItemsNoMatch() {
        val json =
                """
            {
                "OutageItems": [
                    { "Message": "Maintenance work in progress" },
                    { "Message": "Other outage" }
                ]
            }
        """.trimIndent()

        val count = provider.parseOutageItems(json, "Kuźnicza", null)
        assertEquals(0, count)
    }

    @Test
    fun testParseOutageItemsCompoundMatch() {
        val json =
                """
            {
                "OutageItems": [
                    { "Message": "Outage at Berga Maxa 12" },
                    { "Message": "Outage at Kolberga 5" }
                ]
            }
        """.trimIndent()

        // "Berga" streetName1, "Maxa" streetName2
        // compound "Maxa Berga" doesn't match "Berga Maxa" (wrong order)
        // but word "Berga" matches "Berga Maxa" (whole word)
        // and word "Berga" does NOT match "Kolberga" (word boundary)
        val count = provider.parseOutageItems(json, "Berga", "Maxa")
        assertEquals(1, count)
    }

    @Test
    fun testParseOutageItemsWithDates() {
        val now = java.util.Date()
        val format = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", java.util.Locale.US)
        format.timeZone = java.util.TimeZone.getTimeZone("UTC")

        val pastDate = format.format(java.util.Date(now.time - 3600000))
        val futureDate = format.format(java.util.Date(now.time + 3600000))

        val json =
                """
            {
                "OutageItems": [
                    { "Message": "Past Outage at Kuźnicza 1", "EndDate": "${pastDate}" },
                    { "Message": "Future Outage at Kuźnicza 2", "EndDate": "${futureDate}" },
                    { "Message": "Outage No Date at Kuźnicza 3" }
                ]
            }
        """.trimIndent()

        val count = provider.parseOutageItems(json, "Kuźnicza", null)
        assertEquals(2, count)
    }

    @Test
    fun testParseMpwikItems() {
        val now = java.util.Date()
        val format = java.text.SimpleDateFormat("dd-MM-yyyy HH:mm", java.util.Locale.getDefault())

        val pastDate = format.format(java.util.Date(now.time - 3600000))
        val futureDate = format.format(java.util.Date(now.time + 3600000))

        val json =
                """
            {
                "failures": [
                    { "content": "Water outage at Gajowicka", "date_end": "${futureDate}" },
                    { "content": "Maintenance at Kuźnicza", "date_end": "${futureDate}" },
                    { "content": "Old work at Gajowicka", "date_end": "${pastDate}" }
                ]
            }
        """.trimIndent()

        val count = provider.parseMpwikItems(json, "Gajowicka", null)
        assertEquals(1, count)

        val count2 = provider.parseMpwikItems(json, "Kuźnicza", null)
        assertEquals(1, count2)
    }
}
