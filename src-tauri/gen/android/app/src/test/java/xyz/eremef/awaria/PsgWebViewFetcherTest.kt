package xyz.eremef.awaria

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class PsgWebViewFetcherTest {

    private fun createSettings(
        cityName: String = "",
        streetName1: String = "",
        streetName2: String? = null
    ): WidgetSettings {
        return WidgetSettings(
            name = "Test",
            cityName = cityName,
            voivodeship = "Lubuskie",
            district = "Wschowski",
            commune = "Wschowa",
            streetName = if (streetName2 != null) "$streetName2 $streetName1" else streetName1,
            streetName1 = streetName1,
            streetName2 = streetName2,
            houseNo = "1",
            cityId = 1,
            streetId = 1,
            theme = "system",
            language = "pl",
            isActive = true,
            sourceEnabled = true
        )
    }

    @Test
    fun testWschowaActiveOutagesMatch() {
        val outages = listOf(
            PsgWebViewFetcher.PsgOutage(
                province = "Lubuskie",
                city = "Wschowa",
                area = "Wschowa ul 31 – go Stycznia 1-19, ul Grunwaldu 1,3 ul Wolsztyńska 11, 13,15.",
                startDate = "17.05.2026 godz. 17:30",
                endDate = "termin zostanie podany wkrótce",
                info = "awaria",
                type = "awaria",
                status = "aktywna"
            )
        )

        // Address 1: ul. Wolsztyńska 1, Wschowa
        val settings1 = listOf(createSettings(cityName = "Wschowa", streetName1 = "Wolsztyńska", streetName2 = "ul."))
        val count1 = PsgWebViewFetcher.countMatchingOutages(outages, settings1)
        assertEquals("Wolsztyńska address should match", 1, count1)

        // Address 2: pl. Plac Grunwaldu 1, Wschowa
        val settings2 = listOf(createSettings(cityName = "Wschowa", streetName1 = "Plac Grunwaldu", streetName2 = "pl."))
        val count2 = PsgWebViewFetcher.countMatchingOutages(outages, settings2)
        assertEquals("Plac Grunwaldu address should match", 1, count2)

        // Non-matching address: ul. Legnicka, Wschowa
        val settings3 = listOf(createSettings(cityName = "Wschowa", streetName1 = "Legnicka", streetName2 = "ul."))
        val count3 = PsgWebViewFetcher.countMatchingOutages(outages, settings3)
        assertEquals("Legnicka address should not match", 0, count3)
    }

    @Test
    fun testHouseNumberFiltering() {
        val outages = listOf(
            PsgWebViewFetcher.PsgOutage(
                province = "Lubuskie",
                city = "Wschowa",
                area = "Wschowa ul Wolsztyńska 10-20 parzyste, ul Grunwaldzka 1, 3, 5, ul Kolejowa 11-21 nieparz.",
                startDate = "17.05.2026 godz. 17:30",
                endDate = "2026-06-03 12:00:00",
                info = "awaria",
                type = "awaria",
                status = "aktywna"
            )
        )

        // 1. In even range, matching house number: 12
        val s1 = WidgetSettings(
            name = "Test", cityName = "Wschowa", voivodeship = "Lubuskie", district = "Wschowski", commune = "Wschowa",
            streetName = "Wolsztyńska", streetName1 = "Wolsztyńska", streetName2 = null,
            houseNo = "12", cityId = 1, streetId = 1, theme = "system", language = "pl",
            isActive = true, sourceEnabled = true, filterByHouseNo = true
        )
        assertEquals(1, PsgWebViewFetcher.countMatchingOutages(outages, listOf(s1)))

        // 2. In even range, odd house number (mismatch due to parity): 13
        val s2 = s1.copy(houseNo = "13")
        assertEquals(0, PsgWebViewFetcher.countMatchingOutages(outages, listOf(s2)))

        // 3. Out of even range, even house number: 22
        val s3 = s1.copy(houseNo = "22")
        assertEquals(0, PsgWebViewFetcher.countMatchingOutages(outages, listOf(s3)))

        // 4. Standalone list, matching house number: 3A (letters ignored, matches 3)
        val s4 = s1.copy(streetName = "Grunwaldzka", streetName1 = "Grunwaldzka", houseNo = "3A")
        assertEquals(1, PsgWebViewFetcher.countMatchingOutages(outages, listOf(s4)))

        // 5. Standalone list, mismatch: 2
        val s5 = s4.copy(houseNo = "2")
        assertEquals(0, PsgWebViewFetcher.countMatchingOutages(outages, listOf(s5)))

        // 6. With filterByHouseNo = false, even mismatching house number should match (behaves as street-wide)
        val s6 = s2.copy(filterByHouseNo = false)
        assertEquals(1, PsgWebViewFetcher.countMatchingOutages(outages, listOf(s6)))
    }
}
