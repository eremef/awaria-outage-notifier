package xyz.eremef.awaria

import android.content.Context
import android.util.Log
import java.net.URL
import java.text.SimpleDateFormat
import java.util.*

class PgeProvider : IOutageProvider {
    companion object {
        private const val TAG = "AwariaPge"
    }

    override val id: String = "pge"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int {
        val relevantSettings = settingsList.filter { isInPgeRegion(it) }
        if (relevantSettings.isEmpty()) return 0

        var totalCount = 0
        try {
            val now = Date()
            val sdf = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US)
            val stopAtFrom = sdf.format(now).replace(" ", "+").replace(":", "%3A")
            val future = Date(now.time + 90L * 24 * 60 * 60 * 1000) // 90 days
            val startAtTo = sdf.format(future).replace(" ", "+").replace(":", "%3A")

            val urlString =
                "https://power-outage.gkpge.pl/api/power-outage?startAtTo=$startAtTo&stopAtFrom=$stopAtFrom&types[]=2"
            val response = WidgetUtils.fetchJson(URL(urlString))
            val outages = org.json.JSONArray(response)

            for (settings in relevantSettings) {
                var count = 0
                for (i in 0 until outages.length()) {
                    val outage = outages.getJSONObject(i)
                    
                    // Redundant local check for safety
                    val stopAtStr = outage.optString("stopAt", "")
                    if (!DateUtils.isOutageActive(stopAtStr)) continue

                    val description = outage.optString("description", "")
                    val cityMatchDesc =
                        description.lowercase().contains(settings.cityName.lowercase())
                    val addresses = outage.optJSONArray("addresses") ?: continue
                    var matched = false
                    for (j in 0 until addresses.length()) {
                        val addr = addresses.getJSONObject(j)
                        val teryt = addr.optJSONObject("teryt")
                        if (teryt != null) {
                            val vMatch =
                                teryt.optString("voivodeshipName").uppercase() ==
                                        settings.voivodeship.uppercase()
                            if (!vMatch) continue

                            val dMatch =
                                teryt.optString("countyName").lowercase() ==
                                        settings.district.lowercase()
                            if (!dMatch) continue

                            val cMatch =
                                teryt.optString("communeName").lowercase() ==
                                        settings.commune.lowercase()
                            if (!cMatch) continue

                            val cityMatch =
                                teryt.optString("cityName").lowercase() ==
                                        settings.cityName.lowercase()
                            if (!cityMatch) continue

                            val streetQuery = settings.streetName1.lowercase()
                            val streetMatch =
                                if (streetQuery.isEmpty()) true
                                else
                                    teryt.optString("streetName")
                                        .lowercase()
                                        .contains(streetQuery)

                            if (streetMatch) {
                                matched = true
                                break
                            }
                        } else if (cityMatchDesc) {
                            if (settings.streetName1.isEmpty()) {
                                matched = true
                                break
                            }
                            if (description.lowercase().contains(settings.streetName1.lowercase())
                            ) {
                                matched = true
                                break
                            }
                        }
                    }
                    if (matched) count++
                }
                totalCount += count
            }
        } catch (e: Exception) {
            Log.e(TAG, "PGE sync failed", e)
        }
        return totalCount
    }

    private fun isInPgeRegion(settings: WidgetSettings): Boolean {
        val v = settings.voivodeship.lowercase()
        return v.contains("lubelskie") ||
                v.contains("podlaskie") ||
                v.contains("łódzkie") ||
                v.contains("świętokrzyskie") ||
                v.contains("mazowieckie") ||
                v.contains("małopolskie") ||
                v.contains("podkarpackie")
    }
}
