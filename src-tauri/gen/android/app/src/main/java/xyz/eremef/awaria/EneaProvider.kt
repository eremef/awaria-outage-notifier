package xyz.eremef.awaria

import android.content.Context
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import java.net.HttpURLConnection
import java.net.URL

class EneaProvider : IOutageProvider {
    companion object {
        private const val TAG = "AwariaEnea"
    }

    override val id: String = "enea"

    override suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int =
        coroutineScope {
            var totalCount = 0
            try {
                val targetRegions =
                    settingsList
                        .flatMap { getEneaRegionsForDistrict(it.district) }
                        .distinct()
                if (targetRegions.isEmpty()) return@coroutineScope 0

                val jobs =
                    targetRegions.map { id ->
                        async(Dispatchers.IO) {
                            try {
                                val url =
                                    URL(
                                        "https://www.wylaczenia-eneaoperator.pl/rss/rss_unpl_$id.xml"
                                    )
                                val conn = url.openConnection() as HttpURLConnection
                                conn.requestMethod = "GET"
                                conn.connectTimeout = 10000
                                conn.readTimeout = 10000
                                if (conn.responseCode in 200..299) {
                                    conn.inputStream.bufferedReader().use { it.readText() }
                                } else null
                            } catch (e: Exception) {
                                null
                            }
                        }
                    }

                val xmls = jobs.awaitAll().filterNotNull()
                val titleRegex = Regex("<title>(.*?)</title>")
                val descriptionRegex =
                    Regex("<description>(.*?)</description>", RegexOption.DOT_MATCHES_ALL)
                // Regex to match "YYYY-MM-DD HH:MM - YYYY-MM-DD HH:MM"
                val dateRangeRegex = Regex("(\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}) - (\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2})")

                    val matchers = settingsList.map { it to WidgetUtils.CompiledMatcher(it) }
                    val counts = IntArray(settingsList.size) { 0 }

                    for (xml in xmls) {
                        val items = xml.split("<item>").drop(1)
                        for (itemXml in items) {
                            val descMatch = descriptionRegex.find(itemXml)
                            if (descMatch != null) {
                                var description = descMatch.groupValues[1]
                                description =
                                    description
                                        .trim()
                                        .removePrefix("<![CDATA[")
                                        .removeSuffix("]]>")
                                        .trim()
                                
                                for (idx in matchers.indices) {
                                    val (settings, matcher) = matchers[idx]
                                    // Enea description contains both city and street
                                    if (matcher.matchesCity(description) && matcher.matchesStreet(description)) {
                                        counts[idx]++
                                    }
                                }
                            }
                        }
                    }
                    totalCount = counts.sum()
            } catch (e: Exception) {
                Log.e(TAG, "Enea sync failed", e)
            }
            totalCount
        }

    private fun getEneaRegionsForDistrict(district: String): List<Int> {
        val d = district.lowercase().removePrefix("m. ")
        return when (d) {
            "zielonogórski", "zielona góra" -> listOf(1)
            "żarski", "żagański" -> listOf(2)
            "wolsztyński" -> listOf(3)
            "świebodziński" -> listOf(4)
            "nowosolski", "wschowski" -> listOf(5)
            "krośnieński" -> listOf(6)
            "poznański", "poznań", "śremski", "obornicki" -> listOf(7)
            "wałecki" -> listOf(8)
            "wrzesiński", "słupecki", "średzki" -> listOf(9)
            "szamotulski" -> listOf(10)
            "pilski", "piła", "złotowski" -> listOf(11)
            "nowotomyski", "grodziski" -> listOf(12)
            "leszczyński", "leszno", "gostyński", "rawicki" -> listOf(13)
            "kościański" -> listOf(14)
            "gnieźnieński", "gniezno" -> listOf(15)
            "chodzieski", "czarnkowsko-trzcianecki" -> listOf(16)
            "bydgoski", "bydgoszcz" -> listOf(17)
            "świecki", "chełmiński", "tucholski" -> listOf(18)
            "nakielski", "sępoleński" -> listOf(19)
            "mogileński", "żniński" -> listOf(20)
            "inowrocławski", "inowrocław" -> listOf(21)
            "chojnicki", "człuchowski" -> listOf(22)
            "szczeciński", "szczecin", "policki" -> listOf(23)
            "stargardzki", "pyrzycki", "stargard" -> listOf(24)
            "kamieński", "świnoujście" -> listOf(25)
            "gryficki", "łobeski" -> listOf(26)
            "goleniowski" -> listOf(27)
            "gorzowski", "gorzów wlkp.", "gorzów wielkopolski", "strzelecko-drezdenecki" ->
                listOf(28)
            "sulęciński", "słubicki" -> listOf(29)
            "międzychodzki" -> listOf(30)
            "myśliborski" -> listOf(31)
            "choszczeński" -> listOf(32)
            else -> emptyList()
        }
    }
}
