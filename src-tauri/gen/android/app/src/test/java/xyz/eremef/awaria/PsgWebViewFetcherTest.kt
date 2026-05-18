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
}
