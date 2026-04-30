package xyz.eremef.awaria

import android.appwidget.AppWidgetManager
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowManager
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.ListView
import android.widget.ProgressBar
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import kotlinx.coroutines.*

class WidgetConfigActivity : ComponentActivity() {
    private var appWidgetId = AppWidgetManager.INVALID_APPWIDGET_ID
    private var fullAddresses = listOf<WidgetSettings>()

    override fun onCreate(savedInstanceState: Bundle?) {
        // Enable edge-to-edge for Android 15+ (API 35+) compatibility.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT)
        )
        
        // Set layout in display cutout mode to use the full screen including notch areas.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }

        super.onCreate(savedInstanceState)
        
        // Default to canceled
        setResult(RESULT_CANCELED)
        
        setContentView(R.layout.widget_config)

        val intent = intent
        val extras = intent.extras
        if (extras != null) {
            appWidgetId = extras.getInt(
                AppWidgetManager.EXTRA_APPWIDGET_ID,
                AppWidgetManager.INVALID_APPWIDGET_ID
            )
        }

        if (appWidgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
            finish()
            return
        }

        val appWidgetManager = AppWidgetManager.getInstance(this)
        val info = appWidgetManager.getAppWidgetInfo(appWidgetId)
        val provider = getProviderForWidget(info?.provider?.className ?: "")

        val addressNames = loadAddressNames(provider)
        val adapter = ArrayAdapter(this, android.R.layout.simple_list_item_single_choice, addressNames)
        val listView = findViewById<ListView>(R.id.address_list)
        listView.adapter = adapter

        val btnConfirm = findViewById<Button>(R.id.btn_confirm)
        val btnCancel = findViewById<Button>(R.id.btn_cancel)
        val progressBar = findViewById<ProgressBar>(R.id.config_progress)

        // Pre-select current address if editing
        val currentAddressId = BaseWidgetProvider.getStoredAddressId(this, appWidgetId)
        if (currentAddressId == null) {
            // No custom address means "Follow Primary" (index 0)
            listView.setItemChecked(0, true)
            btnConfirm.isEnabled = true
        } else {
            // Find which address matches the stored ID
            for (i in fullAddresses.indices) {
                val addr = fullAddresses[i]
                val id = "${addr.cityId}-${addr.streetId}-${addr.houseNo}"
                if (id == currentAddressId) {
                    listView.setItemChecked(i + 1, true)
                    btnConfirm.isEnabled = true
                    listView.setSelection(i + 1)
                    break
                }
            }
        }

        listView.setOnItemClickListener { _, _, _, _ ->
            btnConfirm.isEnabled = true
        }

        btnCancel.setOnClickListener {
            finish()
        }

        btnConfirm.setOnClickListener {
            val position = listView.checkedItemPosition
            if (position == ListView.INVALID_POSITION) return@setOnClickListener

            // Disable UI to prevent multiple clicks
            btnConfirm.isEnabled = false
            btnConfirm.visibility = View.INVISIBLE
            progressBar.visibility = View.VISIBLE
            listView.isEnabled = false

            val context = this@WidgetConfigActivity
            
            if (position == 0) {
                // "Follow Primary Address" - delete custom mapping
                BaseWidgetProvider.deleteAddressId(context, appWidgetId)
            } else {
                val selectedAddress = fullAddresses[position - 1]
                val addressId = "${selectedAddress.cityId}-${selectedAddress.streetId}-${selectedAddress.houseNo}"
                BaseWidgetProvider.saveAddressId(context, appWidgetId, addressId)
            }

            // Success result
            val resultValue = Intent().apply {
                putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
            }
            setResult(RESULT_OK, resultValue)

            // Trigger an immediate update and finish
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    provider.updateWidget(context, appWidgetManager, appWidgetId)
                } finally {
                    withContext(Dispatchers.Main) {
                        finish()
                    }
                }
            }
        }
    }

    private fun getProviderForWidget(className: String): BaseWidgetProvider {
        return when {
            className.contains("AllWidgetProvider") -> AllWidgetProvider()
            className.contains("TauronWidgetProvider") -> TauronWidgetProvider()
            className.contains("EnergaWidgetProvider") -> EnergaWidgetProvider()
            className.contains("EneaWidgetProvider") -> EneaWidgetProvider()
            className.contains("PgeWidgetProvider") -> PgeWidgetProvider()
            className.contains("StoenWidgetProvider") -> StoenWidgetProvider()
            className.contains("FortumWidgetProvider") -> FortumWidgetProvider()
            className.contains("MpwikWidgetProvider") -> MpwikWidgetProvider()
            className.contains("PsgWidgetProvider") -> PsgWidgetProvider()
            else -> TriWidgetProvider()
        }
    }

    private fun loadAddressNames(provider: BaseWidgetProvider): List<String> {
        val names = mutableListOf<String>()
        names.add(getString(R.string.config_primary_address))
        
        val settingsList = provider.loadSettings(this)
        if (settingsList != null) {
            // Only show active addresses
            val activeSettings = settingsList.filter { it.isActive }
            fullAddresses = activeSettings
            for (ws in activeSettings) {
                val displayName = if (ws.name.isNotEmpty()) ws.name else "${ws.cityName}, ${ws.streetName} ${ws.houseNo}"
                names.add(displayName)
            }
        }
        
        return names
    }
}
