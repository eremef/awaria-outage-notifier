package xyz.eremef.awaria

import android.content.Context
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class TauronProvider : IOutageProvider {
    override val id: String = "tauron"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        var totalCount = 0
        for (settings in settingsList) {
            try {
                // Look up city GAID
                val cityGAID = lookupTauronCity(settings) ?: continue

                // Check for city without streets (TERYT streetId == 0)
                val streetGAID =
                    if (settings.streetName1.isEmpty()) {
                        lookupTauronStreetDummy(cityGAID)
                    } else {
                        val streetQuery = settings.streetName1
                        lookupTauronStreet(streetQuery, cityGAID)
                    }
                    ?: continue

                // Fetch outages
                val dateFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
                dateFormat.timeZone = TimeZone.getTimeZone("UTC")
                val now = dateFormat.format(Date())
                val baseUrl = "https://www.tauron-dystrybucja.pl/waapi/outages/address"
                val safeHouseNo =
                    java.net.URLEncoder.encode(settings.houseNo, "utf-8").replace("+", "%20")
                val params =
                    "cityGAID=$cityGAID&streetGAID=$streetGAID" +
                            "&houseNo=$safeHouseNo" +
                            "&fromDate=$now&getLightingSupport=false" +
                            "&getServicedSwitchingoff=true&_=${System.currentTimeMillis()}"

                val url = URL("$baseUrl?$params")
                val conn = url.openConnection() as HttpURLConnection
                conn.requestMethod = "GET"
                conn.setRequestProperty("accept", "application/json")
                conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
                conn.setRequestProperty("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
                conn.connectTimeout = 10000
                conn.readTimeout = 10000

                val responseCode = conn.responseCode
                if (responseCode !in 200..299) {
                    conn.disconnect()
                    continue
                }

                val response = conn.inputStream.bufferedReader().readText()
                conn.disconnect()

                totalCount += parseOutageItems(response, settings)
            } catch (e: Exception) {
                continue
            }
        }
        return totalCount
    }

    private fun lookupTauronCity(settings: WidgetSettings): Long? {
        return try {
            val encoded = java.net.URLEncoder.encode(settings.cityName, "utf-8").replace("+", "%20")
            val url =
                URL(
                    "https://www.tauron-dystrybucja.pl/waapi/enum/geo/cities?partName=$encoded&_=${System.currentTimeMillis()}"
                )
            val conn = url.openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("accept", "application/json")
            conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
            conn.setRequestProperty("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
            conn.connectTimeout = 10000
            conn.readTimeout = 10000

            val responseCode = conn.responseCode
            if (responseCode !in 200..299) {
                conn.disconnect()
                return null
            }

            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            val items = org.json.JSONArray(response)
            for (i in 0 until items.length()) {
                val item = items.getJSONObject(i)
                val prov = item.optString("ProvinceName", "")
                val dist = item.optString("DistrictName", "")
                val comm = item.optString("CommuneName", "")

                if (prov.equals(settings.voivodeship, ignoreCase = true) &&
                    dist.equals(settings.district, ignoreCase = true) &&
                    comm.equals(settings.commune, ignoreCase = true)
                ) {
                    return item.getLong("GAID")
                }
            }
            if (items.length() > 0) items.getJSONObject(0).getLong("GAID") else null
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronStreetDummy(cityGAID: Long): Long? {
        return try {
            val url =
                URL(
                    "https://www.tauron-dystrybucja.pl/waapi/enum/geo/onlyonestreet?ownerGAID=$cityGAID&_=${System.currentTimeMillis()}"
                )
            val conn = url.openConnection() as HttpURLConnection
            conn.connectTimeout = 10000
            conn.readTimeout = 10000
            if (conn.responseCode !in 200..299) {
                conn.disconnect()
                return null
            }
            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()
            val items = org.json.JSONArray(response)
            if (items.length() > 0) items.getJSONObject(0).getLong("GAID") else null
        } catch (e: Exception) {
            null
        }
    }

    private fun lookupTauronStreet(streetName: String, cityGAID: Long): Long? {
        return try {
            val encoded = java.net.URLEncoder.encode(streetName, "utf-8").replace("+", "%20")
            val url =
                URL(
                    "https://www.tauron-dystrybucja.pl/waapi/enum/geo/streets?partName=$encoded&ownerGAID=$cityGAID&_=${System.currentTimeMillis()}"
                )
            val conn = url.openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.setRequestProperty("accept", "application/json")
            conn.setRequestProperty("x-requested-with", "XMLHttpRequest")
            conn.setRequestProperty("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
            conn.connectTimeout = 10000
            conn.readTimeout = 10000

            val responseCode = conn.responseCode
            if (responseCode !in 200..299) {
                conn.disconnect()
                return null
            }

            val response = conn.inputStream.bufferedReader().readText()
            conn.disconnect()

            val json = JSONObject("{\"items\":$response}")
            val items = json.optJSONArray("items") ?: return null
            if (items.length() == 0) return null
            items.getJSONObject(0).getLong("GAID")
        } catch (e: Exception) {
            null
        }
    }

    private fun parseOutageItems(jsonString: String, settings: WidgetSettings): Int {
        val json = JSONObject(jsonString)
        val items = json.optJSONArray("OutageItems") ?: return 0
        var count = 0
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val endDateStr = item.optString("EndDate", "")
            if (!DateUtils.isOutageActive(endDateStr)) continue
            val message = item.optString("Message", "")
            if (WidgetUtils.matchesStreetOnly(message, settings.streetName1, settings.streetName2)) count++
        }
        return count
    }
}
