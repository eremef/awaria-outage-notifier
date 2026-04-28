package xyz.eremef.awaria

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.*

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE)
class DateUtilsTest {

    @Test
    fun testParseDateTStandard() {
        val date = DateUtils.parseDate("2024-02-12T10:30:00.000Z")
        assertNotNull(date)
        val calendar = Calendar.getInstance(TimeZone.getTimeZone("UTC"))
        calendar.time = date!!
        assertEquals(2024, calendar.get(Calendar.YEAR))
        assertEquals(1, calendar.get(Calendar.MONTH)) // Feb is 1
        assertEquals(12, calendar.get(Calendar.DAY_OF_MONTH))
        assertEquals(10, calendar.get(Calendar.HOUR_OF_DAY))
    }

    @Test
    fun testParseDateFallbacks() {
        // Space instead of T
        assertNotNull(DateUtils.parseDate("2024-02-12 10:30:00"))
        
        // No seconds
        assertNotNull(DateUtils.parseDate("2024-02-12 10:30"))
        
        // Polish format
        assertNotNull(DateUtils.parseDate("12-02-2024 10:30"))
        
        // Only date
        assertNotNull(DateUtils.parseDate("2024-02-12"))
    }

    @Test
    fun testParseDateInvalid() {
        assertNull(DateUtils.parseDate("not-a-date"))
        assertNull(DateUtils.parseDate(""))
        assertNull(DateUtils.parseDate(null))
        // Special case from PSG
        assertNull(DateUtils.parseDate("termin zostanie podany wkrótce"))
    }

    @Test
    fun testIsOutageActive() {
        val now = Date()
        val future = Date(now.time + 3600000)
        val past = Date(now.time - 3600000)
        
        val format = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.US)
        format.timeZone = TimeZone.getTimeZone("UTC")
        
        assertTrue(DateUtils.isOutageActive(format.format(future)))
        assertFalse(DateUtils.isOutageActive(format.format(past)))
        
        // Missing date should be considered active (safer fallback)
        assertTrue(DateUtils.isOutageActive(null))
        assertTrue(DateUtils.isOutageActive(""))
        assertTrue(DateUtils.isOutageActive("invalid-rendering"))
    }
}
