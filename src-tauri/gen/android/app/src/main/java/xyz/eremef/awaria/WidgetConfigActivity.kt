package xyz.eremef.awaria

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.util.TypedValue
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.doAfterTextChanged
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import com.google.android.material.button.MaterialButton
import com.google.android.material.card.MaterialCardView
import com.google.android.material.chip.Chip
import com.google.android.material.chip.ChipGroup

class WidgetConfigActivity : ComponentActivity() {
    private enum class WidgetKind { SINGLE, TRI, ALL }

    private var appWidgetId = AppWidgetManager.INVALID_APPWIDGET_ID
    private var widgetKind: WidgetKind? = null
    private var widgetProvider: BaseWidgetProvider? = null
    private var settingsResult: Pair<List<WidgetSettings>, org.json.JSONObject?>? = null
    private var enabledSources = emptySet<String>()
    private var sourceStatusKnown = false
    private var selectedProviderId: String? = null
    private var selectedAddressKey: String? = null
    private var selectedUtility: WidgetUtility? = null
    private var searchQuery = ""
    private var isReconfiguration = false
    private var saving = false

    private lateinit var providerSearch: com.google.android.material.textfield.TextInputEditText
    private lateinit var filterGroup: ChipGroup
    private lateinit var enabledRows: LinearLayout
    private lateinit var otherRows: LinearLayout
    private lateinit var addressRows: LinearLayout
    private lateinit var emptyResults: TextView
    private lateinit var resetSearch: MaterialButton
    private lateinit var selectedCard: MaterialCardView
    private lateinit var selectedHeading: TextView
    private lateinit var enabledHeading: TextView
    private lateinit var otherHeading: TextView
    private lateinit var addressHelp: TextView
    private lateinit var configError: TextView
    private lateinit var confirmButton: MaterialButton
    private lateinit var progress: View

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setResult(RESULT_CANCELED)
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        setContentView(R.layout.widget_config)
        applyInsets()
        bindViews()

        val requestedId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        val manager = AppWidgetManager.getInstance(this)
        val info = if (requestedId != AppWidgetManager.INVALID_APPWIDGET_ID) manager.getAppWidgetInfo(requestedId) else null
        if (info == null || info.provider == null || info.provider.packageName != packageName) {
            finish()
            return
        }
        appWidgetId = requestedId
        when (info.provider.className) {
            ComponentName(this, SingleWidgetProvider::class.java).className -> {
                widgetKind = WidgetKind.SINGLE
                widgetProvider = SingleWidgetProvider()
            }
            ComponentName(this, TriWidgetProvider::class.java).className -> {
                widgetKind = WidgetKind.TRI
                widgetProvider = TriWidgetProvider()
            }
            ComponentName(this, AllWidgetProvider::class.java).className -> {
                widgetKind = WidgetKind.ALL
                widgetProvider = AllWidgetProvider()
            }
            else -> {
                finish()
                return
            }
        }

        val provider = widgetProvider ?: run { finish(); return }
        settingsResult = if (widgetKind == WidgetKind.SINGLE) {
            val current = SingleWidgetConfigStore.get(this, appWidgetId)
            current?.let { isReconfiguration = true }
            provider.loadSettings(this)
        } else {
            provider.loadSettings(this)
        }
        updateEnabledSources()

        if (savedInstanceState != null) {
            isReconfiguration = savedInstanceState.getBoolean(STATE_RECONFIGURATION, isReconfiguration)
            selectedProviderId = savedInstanceState.getString(STATE_PROVIDER)
            selectedAddressKey = savedInstanceState.getString(STATE_ADDRESS)
            searchQuery = savedInstanceState.getString(STATE_QUERY).orEmpty()
            selectedUtility = savedInstanceState.getString(STATE_UTILITY)?.let { value ->
                runCatching { WidgetUtility.valueOf(value) }.getOrNull()
            }
        } else if (widgetKind == WidgetKind.SINGLE) {
            val config = SingleWidgetConfigStore.get(this, appWidgetId)
            selectedProviderId = config?.providerId?.takeIf { SingleProviderCatalog.find(it) != null }
            selectedAddressKey = when (config?.addressMode) {
                WidgetAddressMode.EXPLICIT -> config.addressId
                else -> ADDRESS_PRIMARY
            }
        } else {
            selectedAddressKey = BaseWidgetProvider.getStoredAddressId(this, appWidgetId) ?: ADDRESS_PRIMARY
            isReconfiguration = BaseWidgetProvider.hasWidgetConfiguration(this, appWidgetId) ||
                BaseWidgetProvider.getStoredAddressId(this, appWidgetId) != null
        }

        providerSearch.setText(searchQuery)
        setTitleAndDescription()
        if (widgetKind == WidgetKind.SINGLE) {
            configureProviderPicker()
            configureAddressChoices(singleProvider = true)
        } else {
            hideProviderPicker()
            configureAddressChoices(singleProvider = false)
        }
        configureActions()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString(STATE_PROVIDER, selectedProviderId)
        outState.putString(STATE_ADDRESS, selectedAddressKey)
        outState.putString(STATE_QUERY, providerSearch.text?.toString().orEmpty())
        outState.putString(STATE_UTILITY, selectedUtility?.name)
        outState.putBoolean(STATE_RECONFIGURATION, isReconfiguration)
        super.onSaveInstanceState(outState)
    }

    private fun applyInsets() {
        val root = findViewById<View>(R.id.config_root)
        val left = root.paddingLeft
        val top = root.paddingTop
        val right = root.paddingRight
        val bottom = root.paddingBottom
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(left + bars.left, top + bars.top, right + bars.right, bottom + bars.bottom)
            insets
        }
        listOf(
            R.id.config_title, R.id.selected_provider_heading, R.id.enabled_providers_heading,
            R.id.other_providers_heading, R.id.address_heading
        ).forEach { ViewCompat.setAccessibilityHeading(findViewById(it), true) }
    }

    private fun bindViews() {
        providerSearch = findViewById(R.id.provider_search)
        filterGroup = findViewById(R.id.provider_filters)
        enabledRows = findViewById(R.id.enabled_provider_rows)
        otherRows = findViewById(R.id.other_provider_rows)
        addressRows = findViewById(R.id.address_rows)
        emptyResults = findViewById(R.id.provider_empty_state)
        resetSearch = findViewById(R.id.reset_provider_search)
        selectedCard = findViewById(R.id.selected_provider_card)
        selectedHeading = findViewById(R.id.selected_provider_heading)
        enabledHeading = findViewById(R.id.enabled_providers_heading)
        otherHeading = findViewById(R.id.other_providers_heading)
        addressHelp = findViewById(R.id.address_help)
        configError = findViewById(R.id.config_error)
        confirmButton = findViewById(R.id.btn_confirm)
        progress = findViewById(R.id.config_progress)
    }

    private fun setTitleAndDescription() {
        findViewById<TextView>(R.id.config_title).text = getString(R.string.widget_config_title)
        findViewById<TextView>(R.id.config_description).text = getString(
            if (widgetKind == WidgetKind.SINGLE) R.string.widget_config_single_description
            else R.string.widget_config_aggregate_description
        )
        confirmButton.text = getString(if (isReconfiguration) R.string.widget_action_save else R.string.widget_action_add)
    }

    private fun hideProviderPicker() {
        listOf(
            R.id.provider_search_layout, R.id.provider_filters, R.id.selected_provider_heading,
            R.id.selected_provider_card, R.id.enabled_providers_heading, R.id.enabled_provider_rows,
            R.id.other_providers_heading, R.id.other_provider_rows, R.id.provider_empty_state,
            R.id.reset_provider_search
        ).forEach { findViewById<View>(it).visibility = View.GONE }
    }

    private fun configureProviderPicker() {
        createFilterChips()
        providerSearch.doAfterTextChanged { editable ->
            searchQuery = editable?.toString().orEmpty()
            renderProviderRows()
            updateConfirmState()
        }
        providerSearch.setOnEditorActionListener { view, actionId, _ ->
            if (actionId == EditorInfo.IME_ACTION_SEARCH) {
                (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager)
                    .hideSoftInputFromWindow(view.windowToken, 0)
                true
            } else false
        }
        findViewById<MaterialButton>(R.id.clear_provider).setOnClickListener {
            selectedProviderId = null
            renderProviderRows()
            updateConfirmState()
        }
        resetSearch.setOnClickListener {
            providerSearch.setText("")
            selectedUtility = null
            createFilterChips()
            renderProviderRows()
            updateConfirmState()
        }
        renderProviderRows()
    }

    private fun createFilterChips() {
        filterGroup.removeAllViews()
        val values = listOf(
            null to R.string.widget_filter_all,
            WidgetUtility.ELECTRICITY to R.string.widget_filter_electricity,
            WidgetUtility.HEAT to R.string.widget_filter_heat,
            WidgetUtility.WATER to R.string.widget_filter_water,
            WidgetUtility.GAS to R.string.widget_filter_gas
        )
        values.forEachIndexed { index, (utility, labelRes) ->
            val chip = Chip(this).apply {
                id = View.generateViewId()
                text = getString(labelRes)
                isCheckable = true
                isChecked = selectedUtility == utility
                tag = utility?.name ?: FILTER_ALL
                contentDescription = text
                minHeight = dp(40)
                setOnClickListener {
                    selectedUtility = utility
                    // Single-selection group updates checked state before this listener.
                    renderProviderRows()
                    updateConfirmState()
                }
            }
            filterGroup.addView(chip)
            if (index == 0 && selectedUtility == null) filterGroup.check(chip.id)
        }
    }

    private fun renderProviderRows() {
        val names = SingleProviderCatalog.entries.associate { it.sourceId to it.displayName(this) }
        val matches = SingleProviderCatalog.search(searchQuery, selectedUtility, enabledSources, names)
        enabledRows.removeAllViews()
        otherRows.removeAllViews()
        matches.enabled.forEach { enabledRows.addView(createProviderRow(it, enabled = true)) }
        matches.other.forEach { otherRows.addView(createProviderRow(it, enabled = false)) }

        val selected = SingleProviderCatalog.find(selectedProviderId)
        selectedHeading.visibility = if (selected != null) View.VISIBLE else View.GONE
        selectedCard.visibility = if (selected != null) View.VISIBLE else View.GONE
        if (selected != null) {
            findViewById<ImageView>(R.id.selected_provider_icon).apply {
                setImageResource(selected.utility.iconRes)
                imageTintList = ColorStateList.valueOf(ContextCompat.getColor(this@WidgetConfigActivity, utilityColor(selected.utility)))
                contentDescription = getString(selected.utility.labelRes)
            }
            findViewById<TextView>(R.id.selected_provider_name).text = selected.displayName(this)
            findViewById<TextView>(R.id.selected_provider_location).text =
                getString(R.string.widget_provider_summary, getString(selected.utility.labelRes), selected.location)
        }
        val noResults = matches.enabled.isEmpty() && matches.other.isEmpty()
        emptyResults.visibility = if (noResults) View.VISIBLE else View.GONE
        resetSearch.visibility = if (noResults) View.VISIBLE else View.GONE
        enabledHeading.visibility = if (matches.enabled.isEmpty()) View.GONE else View.VISIBLE
        otherHeading.visibility = if (matches.other.isEmpty()) View.GONE else View.VISIBLE
        if (!sourceStatusKnown) otherHeading.text = getString(R.string.widget_all_providers)
        else otherHeading.text = getString(R.string.widget_other_providers)
    }

    private fun createProviderRow(entry: SingleProviderEntry, enabled: Boolean): View {
        val card = MaterialCardView(this).apply {
            radius = dp(14).toFloat()
            cardElevation = 0f
            strokeWidth = dp(if (entry.sourceId == selectedProviderId) 2 else 1)
            strokeColor = if (entry.sourceId == selectedProviderId) ContextCompat.getColor(this@WidgetConfigActivity, R.color.widget_selection)
                else ContextCompat.getColor(this@WidgetConfigActivity, R.color.widget_border)
            isFocusable = true
            isClickable = true
            isSelected = entry.sourceId == selectedProviderId
            val status = getString(providerStatusRes(enabled))
            contentDescription = getString(
                R.string.widget_provider_row_description,
                entry.displayName(this@WidgetConfigActivity),
                getString(entry.utility.labelRes),
                entry.location,
                status
            )
            setOnClickListener {
                selectedProviderId = entry.sourceId
                renderProviderRows()
                updateConfirmState()
            }
        }
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(12), dp(10), dp(12), dp(10))
            minimumHeight = dp(64)
            isDuplicateParentStateEnabled = true
        }
        val icon = ImageView(this).apply {
            layoutParams = LinearLayout.LayoutParams(dp(32), dp(32))
            setImageResource(entry.utility.iconRes)
            imageTintList = ColorStateList.valueOf(ContextCompat.getColor(this@WidgetConfigActivity, utilityColor(entry.utility)))
            contentDescription = getString(entry.utility.labelRes)
        }
        row.addView(icon)
        val textColumn = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = dp(12) }
        }
        val title = TextView(this).apply {
            text = entry.displayName(this@WidgetConfigActivity)
            textSize = 16f
            setTextColor(themeColor(android.R.attr.textColorPrimary))
            setTypeface(typeface, Typeface.BOLD)
        }
        val subtitle = TextView(this).apply {
            text = getString(
                R.string.widget_provider_status_location,
                entry.location,
                getString(providerStatusRes(enabled))
            )
            textSize = 13f
            setTextColor(themeColor(android.R.attr.textColorSecondary))
        }
        textColumn.addView(title)
        textColumn.addView(subtitle)
        row.addView(textColumn)
        val utilityLabel = TextView(this).apply {
            text = getString(entry.utility.labelRes)
            textSize = 12f
            setTextColor(ContextCompat.getColor(this@WidgetConfigActivity, utilityColor(entry.utility)))
            contentDescription = getString(entry.utility.labelRes)
        }
        row.addView(utilityLabel)
        card.addView(row)
        card.layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            bottomMargin = dp(8)
        }
        return card
    }

    private fun configureAddressChoices(singleProvider: Boolean) {
        addressRows.removeAllViews()
        configError.visibility = View.GONE
        val addresses = settingsResult?.first.orEmpty()
        val active = addresses.filter { it.isActive }
        val settingsUnavailable = settingsResult == null
        val noActive = !settingsUnavailable && active.isEmpty()
        if (settingsUnavailable) {
            addressHelp.text = getString(R.string.widget_config_settings_unavailable)
            addressHelp.announceForAccessibility(addressHelp.text)
            confirmButton.isEnabled = false
            return
        }
        if (noActive) {
            addressHelp.text = getString(R.string.widget_config_no_active_address)
            confirmButton.isEnabled = false
            return
        }
        addressHelp.text = getString(R.string.widget_address_requires_enabled_provider)
        if (singleProvider) {
            addressRows.addView(createAddressRow(
                ADDRESS_PRIMARY,
                getString(R.string.config_primary_address),
                getString(R.string.widget_address_follow_primary_summary),
                checked = selectedAddressKey == ADDRESS_PRIMARY
            ))
        } else {
            addressRows.addView(createAddressRow(
                ADDRESS_PRIMARY,
                getString(R.string.config_primary_address),
                getString(R.string.widget_address_follow_primary_summary),
                checked = selectedAddressKey == ADDRESS_PRIMARY
            ))
        }

        val identityCounts = addresses.groupingBy(SingleWidgetAddressResolver::addressId).eachCount()
        val visibleAddresses = if (singleProvider) {
            active.filter { identityCounts[SingleWidgetAddressResolver.addressId(it)] == 1 }
        } else active
        visibleAddresses.forEach { address ->
            val id = SingleWidgetAddressResolver.addressId(address)
            val title = address.name.ifBlank { "${address.cityName}, ${address.streetName} ${address.houseNo}" }
            val detail = listOf(address.voivodeship, address.district, address.commune).filter(String::isNotBlank).distinct().joinToString(" · ")
            val firstMatchingId = visibleAddresses.firstOrNull { SingleWidgetAddressResolver.addressId(it) == id } === address
            addressRows.addView(createAddressRow(
                id, title, detail,
                checked = selectedAddressKey == id && (singleProvider || firstMatchingId)
            ))
        }

        if (singleProvider && selectedAddressKey != ADDRESS_PRIMARY &&
            addresses.none { SingleWidgetAddressResolver.addressId(it) == selectedAddressKey && it.isActive && identityCounts[SingleWidgetAddressResolver.addressId(it)] == 1 }) {
            configError.visibility = View.VISIBLE
            configError.text = getString(
                if (addresses.count { SingleWidgetAddressResolver.addressId(it) == selectedAddressKey } > 1)
                    R.string.widget_config_address_ambiguous
                else R.string.widget_config_address_repair
            )
        }
        updateConfirmState()
    }

    private fun createAddressRow(key: String, title: String, detail: String, checked: Boolean): View {
        val card = MaterialCardView(this).apply {
            radius = dp(14).toFloat()
            cardElevation = 0f
            strokeWidth = dp(if (checked) 2 else 1)
            strokeColor = if (checked) ContextCompat.getColor(this@WidgetConfigActivity, R.color.widget_selection)
                else ContextCompat.getColor(this@WidgetConfigActivity, R.color.widget_border)
            isClickable = true
            isFocusable = true
            isSelected = checked
            setOnClickListener {
                selectedAddressKey = key
                configError.visibility = View.GONE
                configureAddressChoices(widgetKind == WidgetKind.SINGLE)
            }
        }
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(12), dp(8), dp(12), dp(8))
            minimumHeight = dp(64)
        }
        val radio = RadioButton(this).apply {
            isChecked = checked
            isClickable = false
            isFocusable = false
            contentDescription = title
        }
        row.addView(radio)
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = dp(6) }
        }
        val titleView = TextView(this).apply {
            text = title
            textSize = 16f
            setTextColor(themeColor(android.R.attr.textColorPrimary))
            setTypeface(typeface, Typeface.BOLD)
        }
        column.addView(titleView)
        if (detail.isNotBlank()) column.addView(TextView(this).apply {
            text = detail
            textSize = 13f
            setTextColor(themeColor(android.R.attr.textColorSecondary))
        })
        row.addView(column)
        card.addView(row)
        card.layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { bottomMargin = dp(8) }
        card.contentDescription = "$title${if (detail.isBlank()) "" else ", $detail"}"
        return card
    }

    private fun configureActions() {
        findViewById<MaterialButton>(R.id.btn_cancel).setOnClickListener { finish() }
        confirmButton.setOnClickListener {
            if (saving) return@setOnClickListener
            if (!isWidgetStillOwned()) {
                finish()
                return@setOnClickListener
            }
            if (!refreshSettingsAndValidateSelection()) return@setOnClickListener
            saving = true
            confirmButton.isEnabled = false
            progress.visibility = View.VISIBLE
            val manager = AppWidgetManager.getInstance(this)
            when (widgetKind) {
                WidgetKind.SINGLE -> {
                    val providerId = selectedProviderId ?: return@setOnClickListener
                    val choice = selectedAddressKey ?: return@setOnClickListener
                    if (!isWidgetStillOwned()) {
                        finish()
                        return@setOnClickListener
                    }
                    val mode = if (choice == ADDRESS_PRIMARY) WidgetAddressMode.FOLLOW_PRIMARY else WidgetAddressMode.EXPLICIT
                    SingleWidgetConfigStore.save(this, appWidgetId, providerId, mode, if (mode == WidgetAddressMode.EXPLICIT) choice else null)
                    val singleProvider = widgetProvider as SingleWidgetProvider
                    singleProvider.showLoadingPlaceholder(this, manager, appWidgetId)
                    queueRefresh()
                }
                WidgetKind.TRI, WidgetKind.ALL -> {
                    val choice = selectedAddressKey ?: return@setOnClickListener
                    if (!isWidgetStillOwned()) {
                        finish()
                        return@setOnClickListener
                    }
                    if (choice == ADDRESS_PRIMARY) BaseWidgetProvider.deleteAddressId(this, appWidgetId)
                    else BaseWidgetProvider.saveAddressId(this, appWidgetId, choice)
                    BaseWidgetProvider.markWidgetConfigured(this, appWidgetId)
                    widgetProvider?.showLoadingPlaceholder(this, manager, appWidgetId)
                    queueRefresh()
                }
                null -> return@setOnClickListener
            }
            val result = Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
            setResult(RESULT_OK, result)
            finish()
        }
        updateConfirmState()
    }

    private fun queueRefresh() {
        val request = OneTimeWorkRequestBuilder<WidgetUpdateWorker>().build()
        WorkManager.getInstance(this).enqueueUniqueWork(
            "${BaseWidgetProvider.WORK_NAME}_$appWidgetId",
            ExistingWorkPolicy.REPLACE,
            request
        )
    }

    private fun isSelectionValid(): Boolean {
        if (settingsResult == null || settingsResult!!.first.none { it.isActive }) return false
        val addressKey = selectedAddressKey ?: return false
        val addresses = settingsResult!!.first
        if (addressKey == ADDRESS_PRIMARY) {
            return addresses.any { it.isActive && it.isPrimary } || addresses.any { it.isActive }
        }
        val matchingAddresses = addresses.filter { SingleWidgetAddressResolver.addressId(it) == addressKey }
        return matchingAddresses.any { it.isActive } &&
            (widgetKind != WidgetKind.SINGLE || matchingAddresses.size == 1)
    }

    private fun refreshSettingsAndValidateSelection(): Boolean {
        settingsResult = widgetProvider?.loadSettings(this)
        updateEnabledSources()
        if (widgetKind == WidgetKind.SINGLE) renderProviderRows()
        configureAddressChoices(widgetKind == WidgetKind.SINGLE)
        val valid = isSelectionValid() &&
            (widgetKind != WidgetKind.SINGLE || SingleProviderCatalog.find(selectedProviderId) != null)
        if (!valid) {
            addressHelp.announceForAccessibility(addressHelp.text)
            updateConfirmState()
        }
        return valid
    }

    private fun isWidgetStillOwned(): Boolean {
        val info = AppWidgetManager.getInstance(this).getAppWidgetInfo(appWidgetId) ?: return false
        val expectedName = when (widgetKind) {
            WidgetKind.SINGLE -> SingleWidgetProvider::class.java.name
            WidgetKind.TRI -> TriWidgetProvider::class.java.name
            WidgetKind.ALL -> AllWidgetProvider::class.java.name
            null -> return false
        }
        return info.provider.packageName == packageName && info.provider.className == expectedName
    }

    private fun updateConfirmState() {
        if (!::confirmButton.isInitialized || saving) return
        val hasProvider = widgetKind != WidgetKind.SINGLE || SingleProviderCatalog.find(selectedProviderId) != null
        confirmButton.isEnabled = hasProvider && isSelectionValid()
    }

    private fun utilityColor(utility: WidgetUtility): Int = when (utility) {
        WidgetUtility.ELECTRICITY -> R.color.utility_power
        WidgetUtility.HEAT -> R.color.utility_heat
        WidgetUtility.WATER -> R.color.utility_water
        WidgetUtility.GAS -> R.color.utility_gas
    }

    private fun updateEnabledSources() {
        val fullJson = settingsResult?.second
        sourceStatusKnown = settingsResult != null
        val allEnabledByDefault = sourceStatusKnown && fullJson?.has("enabledSources") != true
        enabledSources = when {
            allEnabledByDefault -> SingleProviderCatalog.entries.map { it.sourceId }.toSet()
            sourceStatusKnown -> BaseWidgetProvider.getEnabledSources(fullJson)
            else -> emptySet()
        }
    }

    private fun providerStatusRes(enabled: Boolean): Int = when {
        !sourceStatusKnown -> R.string.widget_provider_status_unknown
        enabled -> R.string.widget_provider_enabled
        else -> R.string.widget_provider_inactive
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private fun themeColor(attribute: Int): Int {
        val value = TypedValue()
        if (!theme.resolveAttribute(attribute, value, true)) return ContextCompat.getColor(this, R.color.widget_text_primary)
        if (value.resourceId != 0) {
            return ContextCompat.getColorStateList(this, value.resourceId)?.defaultColor ?: value.data
        }
        return value.data
    }

    companion object {
        private const val ADDRESS_PRIMARY = "__follow_primary__"
        private const val FILTER_ALL = "ALL"
        private const val STATE_PROVIDER = "widget_config.provider"
        private const val STATE_ADDRESS = "widget_config.address"
        private const val STATE_QUERY = "widget_config.query"
        private const val STATE_UTILITY = "widget_config.utility"
        private const val STATE_RECONFIGURATION = "widget_config.reconfiguration"
    }
}
