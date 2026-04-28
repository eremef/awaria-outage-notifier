package xyz.eremef.awaria

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class WidgetUtilsTest {

    private fun createSettings(
        cityName: String = "",
        commune: String = "",
        streetName1: String = "",
        streetName2: String? = null
    ): WidgetSettings {
        return WidgetSettings(
            name = "Test",
            cityName = cityName,
            voivodeship = "Woj",
            district = "Pow",
            commune = commune,
            streetName = "$streetName2 $streetName1",
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
    fun testWordMatchPolishChars() {
        // Test that word boundary [^\p{L}] works for Polish characters
        assertTrue(WidgetUtils.wordMatch("Awaria na ul. Kuźniczej", "Kuźniczej"))
        assertTrue(WidgetUtils.wordMatch("ul. Kuźnicza 12", "Kuźnicza"))
        assertFalse(WidgetUtils.wordMatch("ul. Kuźnicza12", "Kuźnicza")) // No boundary
    }

    @Test
    fun testCompiledMatcherCity() {
        val settings = createSettings(cityName = "Wrocław")
        val matcher = WidgetUtils.CompiledMatcher(settings)
        
        assertTrue(matcher.matchesCity("Outage in Wrocław City"))
        assertTrue(matcher.matchesCity("WROCŁAW - planned works"))
        assertFalse(matcher.matchesCity("In Inowrocław there is no outage")) // Substring match prevention
    }

    @Test
    fun testCompiledMatcherStreetCompound() {
        val settings = createSettings(streetName1 = "Probusa", streetName2 = "Henryka")
        val matcher = WidgetUtils.CompiledMatcher(settings)
        
        // Matches full compound
        assertTrue(matcher.matchesStreet("ul. Henryka Probusa 12"))
        // Matches short name (last part)
        assertTrue(matcher.matchesStreet("Awaria na ul. Probusa"))
        // Does not match similar but different
        assertFalse(matcher.matchesStreet("ul. Legnicka"))
    }

    @Test
    fun testMatchesFull() {
        val settings = createSettings(cityName = "Wrocław", commune = "Wrocław-Stare Miasto", streetName1 = "Kuźnicza")
        val matcher = WidgetUtils.CompiledMatcher(settings)
        
        // Message contains everything
        assertTrue(matcher.matchesFull("Planned outage at Kuźnicza, Wrocław-Stare Miasto, Wrocław"))
        
        // Message contains city and street, areas contains commune
        assertTrue(matcher.matchesFull("Outage at Kuźnicza, Wrocław", listOf("Wrocław-Stare Miasto", "Other Area")))
        
        // Missing commune
        assertFalse(matcher.matchesFull("Outage at Kuźnicza, Wrocław", listOf("Other Area")))
    }

    @Test
    fun testMatchesCityOnly() {
        val settings = createSettings(cityName = "Wrocław", streetName1 = "")
        val matcher = WidgetUtils.CompiledMatcher(settings)
        
        // When no street is configured, it matches if city is in message
        assertTrue(matcher.matchesFull("Brak prądu w mieście Wrocław"))
        assertFalse(matcher.matchesFull("Brak prądu w Opolu"))
    }
}
