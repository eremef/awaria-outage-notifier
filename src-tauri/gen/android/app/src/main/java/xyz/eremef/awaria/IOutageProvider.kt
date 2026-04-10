package xyz.eremef.awaria

import android.content.Context

interface IOutageProvider {
    val id: String
    suspend fun fetchCount(context: Context, settingsList: List<WidgetSettings>): Int
}
