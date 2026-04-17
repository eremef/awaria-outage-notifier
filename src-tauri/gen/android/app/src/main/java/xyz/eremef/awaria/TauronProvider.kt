package xyz.eremef.awaria

import android.content.Context
import kotlinx.coroutines.*
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class TauronProvider : IOutageProvider {
    override val id: String = "tauron"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int = coroutineScope {
        // Parallelize fetching for each address in the list
        val jobs = settingsList.map { settings ->
            async(Dispatchers.IO) {
                try {
                    // 1. Look up city GAID
                    val cityGAID = lookupTauronCity(settings) ?: return@async 0

                    // 2. Check for city without streets or with streets
                    val streetGAID = if (settings.streetName1.isEmpty()) {
                        lookupTauronStreetDummy(cityGAID)
                    } else {
                        lookupTauronStreet(settings.streetName1, cityGAID)
                    } ?: return@async 0

                    // 3. Fetch outages
                    val dateFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
                    dateFormat.timeZone = TimeZone.getTimeZone("UTC")
                    val now = dateFormat.format(Date())
                    val safeHouseNo = settings.houseNo.replace(" ", "%20")
                    
                    val url = URL("https://www.tauron-dystrybucja.pl/waapi/outages/address" +
                            "?cityGAID=$cityGAID&streetGAID=$streetGAID&houseNo=$safeHouseNo" +
                            "&fromDate=$now&getLightingSupport=false&getServicedSwitchingoff=true&_=${System.currentTimeMillis()}")
                    
                    val response = fetchWithHeaders(url) ?: return@async 0
                    val matcher = WidgetUtils.CompiledMatcher(settings)
                    parseOutageItems(response, matcher)
                } catch (e: Exception) {
                    0
                }
            }
        }
        jobs.awaitAll().sum()
    }

    private fun fetchWithHeaders(url: URL): String? {
        return try {
            val conn = url.openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("accept", "application/json")
            conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
            conn.setRequestProperty("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
            conn.connectTimeout = 10000
            conn.readTimeout = 10000
            if (conn.responseCode !in 200..299) {
                conn.disconnect()
                return null
            }
            val res = conn.inputStream.bufferedReader().use { it.readText() }
            conn.disconnect()
            res
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronCity(settings: WidgetSettings): Long? {
        val encoded = java.net.URLEncoder.encode(settings.cityName, "utf-8").replace("+", "%20")
        val url = URL("https://www.tauron-dystrybucja.pl/waapi/enum/geo/cities?partName=$encoded&_=${System.currentTimeMillis()}")
        val response = fetchWithHeaders(url) ?: return null
        
        return try {
            val items = org.json.JSONArray(response)
            
            fun normalize(name: String): String {
                return name.lowercase()
                    .replace(" (miasto)", "")
                    .replace(" m.", "")
                    .replace("powiat ", "")
                    .trim()
            }

            for (i in 0 until items.length()) {
                val item = items.getJSONObject(i)
                val p1 = normalize(item.optString("ProvinceName", ""))
                val p2 = normalize(settings.voivodeship)
                val d1 = normalize(item.optString("DistrictName", ""))
                val d2 = normalize(settings.district)
                val c1 = normalize(item.optString("CommuneName", ""))
                val c2 = normalize(settings.commune)

                if (p1 == p2 && d1 == d2 && c1 == c2) {
                    return item.getLong("GAID")
                }
            }
            if (items.length() > 0) items.getJSONObject(0).getLong("GAID") else null
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronStreetDummy(cityGAID: Long): Long? {
        val url = URL("https://www.tauron-dystrybucja.pl/waapi/enum/geo/onlyonestreet?ownerGAID=$cityGAID&_=${System.currentTimeMillis()}")
        val response = fetchWithHeaders(url) ?: return null
        return try {
            val items = org.json.JSONArray(response)
            if (items.length() > 0) items.getJSONObject(0).getLong("GAID") else null
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronStreet(streetName: String, cityGAID: Long): Long? {
        val encoded = java.net.URLEncoder.encode(streetName, "utf-8").replace("+", "%20")
        val url = URL("https://www.tauron-dystrybucja.pl/waapi/enum/geo/streets?partName=$encoded&ownerGAID=$cityGAID&_=${System.currentTimeMillis()}")
        val response = fetchWithHeaders(url) ?: return null
        return try {
            val json = JSONObject("{\"items\":$response}")
            val items = json.optJSONArray("items") ?: return null
            if (items.length() == 0) return null
            items.getJSONObject(0).getLong("GAID")
        } catch (e: Exception) {
            null
        }
    }

    private fun parseOutageItems(jsonString: String, matcher: WidgetUtils.CompiledMatcher): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("OutageItems") ?: return 0
        var count = 0
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            // We removed isOutageActive check to match Rust backend behavior.
            // API result set from /address endpoint for 'fromDate=now' is already filtered by server.
            val message = item.optString("Message", "")
            if (matcher.matchesStreet(message)) count++
        }
        return count
    }
}
