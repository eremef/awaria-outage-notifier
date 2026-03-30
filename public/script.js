if (typeof document !== 'undefined') {
    document.addEventListener('DOMContentLoaded', () => {
        initSettings();
        initPullToRefresh();
        initRefreshButton();
        initAddressFilter();
        loadSettingsAndFetch();
    });
}

// ── Settings ──────────────────────────────────────────────

let currentSettings = null;
let lastAlerts = [];
let lastFetchDate = null;
let selectedAddressIndex = -1; // -1 means "all addresses"

let selectedCityId = null;
let selectedCityName = '';
let selectedVoivodeship = '';
let selectedDistrict = '';
let selectedCommune = '';
let selectedStreetId = null;
let selectedStreetName = '';
let selectedStreetName1 = '';
let selectedStreetName2 = null;
let cityDebounceTimer = null;
let streetDebounceTimer = null;
let cityHasNoStreets = false;
let editingAddressIndex = null;

function initSettings() {
    const btn = document.getElementById('settings-btn');
    const panel = document.getElementById('settings-panel');
    const saveBtn = document.getElementById('save-settings-btn');
    const themeSelect = document.getElementById('theme-select');
    const langSelect = document.getElementById('language-select');
    const addAddressBtn = document.getElementById('add-address-btn');
    btn.addEventListener('click', () => {
        panel.classList.toggle('hidden');
    });

    saveBtn.addEventListener('click', saveNewAddress);

    addAddressBtn.addEventListener('click', () => {
        document.getElementById('address-form').classList.remove('hidden');
        document.getElementById('add-address-btn').classList.add('hidden');
        document.getElementById('address-name-input').value = '';
        document.getElementById('city-input').value = '';
        document.getElementById('street-input').value = '';
        document.getElementById('street-input').disabled = true;
        document.getElementById('house-input').value = '';
        document.getElementById('settings-status').textContent = '';
        selectedCityId = null;
        selectedCityName = '';
        selectedVoivodeship = '';
        selectedDistrict = '';
        selectedCommune = '';
        selectedStreetId = null;
        selectedStreetName = '';
        selectedStreetName1 = '';
        selectedStreetName2 = null;
        cityHasNoStreets = false;
        editingAddressIndex = null;
        document.getElementById('street-input').classList.remove('grayed-out');
        hideSuggestions('city-suggestions');
        hideSuggestions('street-suggestions');
    });

    document.getElementById('cancel-address-btn').addEventListener('click', function () {
        document.getElementById('address-form').classList.add('hidden');
        document.getElementById('add-address-btn').classList.remove('hidden');
        document.getElementById('addresses-list').classList.remove('hidden');
        document.getElementById('address-name-input').value = '';
        document.getElementById('city-input').value = '';
        document.getElementById('street-input').value = '';
        document.getElementById('street-input').disabled = true;
        document.getElementById('house-input').value = '';
        document.getElementById('settings-status').textContent = '';
        selectedCityId = null;
        selectedCityName = '';
        selectedVoivodeship = '';
        selectedDistrict = '';
        selectedCommune = '';
        selectedStreetId = null;
        selectedStreetName = '';
        selectedStreetName1 = '';
        selectedStreetName2 = null;
        cityHasNoStreets = false;
        document.getElementById('street-input').classList.remove('grayed-out');
        hideSuggestions('city-suggestions');
        hideSuggestions('street-suggestions');
    });

    const cityInput = document.getElementById('city-input');
    cityInput.addEventListener('input', () => {
        selectedCityId = null;
        selectedCityName = '';
        selectedVoivodeship = '';
        selectedDistrict = '';
        selectedCommune = '';
        selectedStreetId = null;
        selectedStreetName = '';
        selectedStreetName1 = '';
        selectedStreetName2 = null;
        document.getElementById('street-input').value = '';
        document.getElementById('street-input').disabled = true;
        cityHasNoStreets = false;
        hideSuggestions('street-suggestions');

        clearTimeout(cityDebounceTimer);
        const query = cityInput.value.trim();
        if (query.length < 2) {
            hideSuggestions('city-suggestions');
            return;
        }
        cityDebounceTimer = setTimeout(() => searchCities(query), 300);
    });

    cityInput.addEventListener('focus', () => {
        if (!selectedCityId && cityInput.value.trim().length >= 2) {
            searchCities(cityInput.value.trim());
        }
    });

    const streetInput = document.getElementById('street-input');
    streetInput.addEventListener('input', () => {
        selectedStreetId = null;
        selectedStreetName = '';
        selectedStreetName1 = '';
        selectedStreetName2 = null;

        clearTimeout(streetDebounceTimer);
        const query = streetInput.value.trim();
        console.log('Street input:', query, 'cityId:', selectedCityId, 'length:', query.length);
        if (query.length < 2 || !selectedCityId) {
            if (query.length >= 2 && !selectedCityId) {
                console.warn('Street typed but no city selected');
            }
            hideSuggestions('street-suggestions');
            return;
        }
        streetDebounceTimer = setTimeout(() => searchStreets(query), 300);
    });

    streetInput.addEventListener('focus', () => {
        if (!selectedStreetId && streetInput.value.trim().length >= 2 && selectedCityId) {
            searchStreets(streetInput.value.trim());
        }
    });

    document.addEventListener('click', (e) => {
        if (!e.target.closest('#city-input') && !e.target.closest('#city-suggestions')) {
            hideSuggestions('city-suggestions');
        }
        if (!e.target.closest('#street-input') && !e.target.closest('#street-suggestions')) {
            hideSuggestions('street-suggestions');
        }
    });

    themeSelect.addEventListener('change', async (e) => {
        const newTheme = e.target.value;
        applyTheme(newTheme);

        if (!currentSettings) {
            currentSettings = {
                addresses: [],
                primaryAddressIndex: null,
                theme: newTheme,
                language: 'system',
                enabledSources: []
            };
        } else {
            currentSettings.theme = newTheme;
        }

        await autoSaveSettings();
        const container = document.getElementById('outages-container');
        renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
        updateLastUpdated();
    });

    langSelect.addEventListener('change', async (e) => {
        const newLang = e.target.value;
        initLanguage(newLang);
        applyTranslations();

        if (!currentSettings) {
            currentSettings = {
                addresses: [],
                primaryAddressIndex: null,
                theme: 'system',
                language: newLang,
                enabledSources: []
            };
        } else {
            currentSettings.language = newLang;
        }

        await autoSaveSettings();
        const container = document.getElementById('outages-container');
        renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
        updateLastUpdated();
    });

    const locTrigger = document.querySelector('#location-settings-collapsible .collapsible-trigger');
    locTrigger.addEventListener('click', () => {
        document.getElementById('location-settings-collapsible').classList.toggle('collapsed');
    });

    ['source-tauron-check', 'source-water-check', 'source-fortum-check', 'source-energa-check', 'source-enea-check', 'source-pge-check', 'source-stoen-check'].forEach(id => {
        const checkbox = document.getElementById(id);
        if (!checkbox) return;
        checkbox.addEventListener('change', async () => {
            if (!currentSettings) return;
            const enabledSources = [];
            if (document.getElementById('source-tauron-check').checked) enabledSources.push('tauron');
            if (document.getElementById('source-water-check').checked) enabledSources.push('water');
            if (document.getElementById('source-fortum-check').checked) enabledSources.push('fortum');
            if (document.getElementById('source-energa-check') && document.getElementById('source-energa-check').checked) enabledSources.push('energa');
            if (document.getElementById('source-enea-check') && document.getElementById('source-enea-check').checked) enabledSources.push('enea');
            if (document.getElementById('source-pge-check') && document.getElementById('source-pge-check').checked) enabledSources.push('pge');
            if (document.getElementById('source-stoen-check') && document.getElementById('source-stoen-check').checked) enabledSources.push('stoen');
            currentSettings.enabledSources = enabledSources;
            await autoSaveSettings();
            fetchOutages();
        });
    });

    ['notify-tauron-check', 'notify-water-check', 'notify-fortum-check', 'notify-energa-check', 'notify-enea-check', 'notify-pge-check', 'notify-stoen-check'].forEach(id => {
        const checkbox = document.getElementById(id);
        if (!checkbox) return;
        checkbox.addEventListener('change', async () => {
            if (!currentSettings) return;
            if (!currentSettings.notificationPreferences) {
                currentSettings.notificationPreferences = {
                    tauron: false,
                    water: false,
                    fortum: false,
                    energa: false,
                    enea: false,
                    pge: false
                };
            }
            const prefKey = id.split('-')[1]; // tauron, water, etc.
            currentSettings.notificationPreferences[prefKey] = checkbox.checked;
            await autoSaveSettings();
        });
    });
}

function initAddressFilter() {
    const filter = document.getElementById('address-filter');
    filter.addEventListener('change', (e) => {
        selectedAddressIndex = parseInt(e.target.value, 10);
        const container = document.getElementById('outages-container');
        renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
    });
}

function updateAddressFilter() {
    const filter = document.getElementById('address-filter');
    const allOpt = filter.querySelector('option[value="-1"]');
    const wasHidden = filter.classList.contains('hidden');
    filter.innerHTML = '';
    filter.appendChild(allOpt);

    const addressCount = currentSettings && currentSettings.addresses ? currentSettings.addresses.length : 0;

    if (addressCount === 0) {
        filter.classList.add('hidden');
    } else if (addressCount === 1) {
        filter.classList.add('hidden');
        selectedAddressIndex = 0;
    } else {
        filter.classList.remove('hidden');
        if (wasHidden) {
            selectedAddressIndex = -1;
            filter.value = '-1';
        }
        currentSettings.addresses.forEach((addr, idx) => {
            const opt = document.createElement('option');
            opt.value = idx;
            opt.textContent = addr.name || `${addr.streetName} ${addr.houseNo}`;
            if (idx === currentSettings.primaryAddressIndex) {
                opt.textContent += ' ⭐';
            }
            filter.appendChild(opt);
        });
    }
}

function renderAddressesList() {
    const list = document.getElementById('addresses-list');
    if (!currentSettings || !currentSettings.addresses || currentSettings.addresses.length === 0) {
        list.innerHTML = `<div class="no-addresses">${typeof t !== 'undefined' ? t('no_addresses') : 'No addresses configured. Add one below.'}</div>`;
        return;
    }

    list.innerHTML = currentSettings.addresses.map((addr, idx) => `
        <div class="address-item">
            <div class="address-info">
                <div class="address-name">${addr.name || 'Address ' + (idx + 1)}</div>
                <div class="address-detail">${addr.streetName} ${addr.houseNo}, ${addr.cityName}</div>
            </div>
            <div class="address-actions">
                ${idx === currentSettings.primaryAddressIndex ? '<span class="primary-badge">⭐</span>' : `<button class="icon-btn" onclick="setPrimaryAddress(${idx})" title="Set as primary">⭐</button>`}
                <button class="icon-btn edit-btn" onclick="editAddress(${idx})" title="Edit">✏️</button>
                <button class="icon-btn delete-btn" onclick="removeAddress(${idx})" title="Remove">🗑️</button>
            </div>
        </div>
    `).join('');
}

window.setPrimaryAddress = async function (idx) {
    try {
        currentSettings = await window.__TAURI__.core.invoke('set_primary_address', { index: idx });
        renderAddressesList();
        updateAddressFilter();
    } catch (error) {
        console.error('Error setting primary address:', error);
    }
};

window.removeAddress = async function (idx) {
    try {
        currentSettings = await window.__TAURI__.core.invoke('remove_address', { index: idx });
        renderAddressesList();
        updateAddressFilter();
        fetchOutages();
    } catch (error) {
        console.error('Error removing address:', error);
    }
};

window.editAddress = function (idx) {
    const addr = currentSettings.addresses[idx];
    if (!addr) return;

    editingAddressIndex = idx;

    // Show form, hide list/add btn
    document.getElementById('address-form').classList.remove('hidden');
    document.getElementById('add-address-btn').classList.add('hidden');
    document.getElementById('addresses-list').classList.add('hidden');

    // Populate fields
    document.getElementById('address-name-input').value = addr.name || '';
    document.getElementById('city-input').value = addr.cityName || '';
    document.getElementById('street-input').value = addr.streetName || '';
    document.getElementById('house-input').value = addr.houseNo || '';

    // Set globals for validation and lookup
    selectedCityId = addr.cityId;
    selectedCityName = addr.cityName;
    selectedVoivodeship = addr.voivodeship || '';
    selectedDistrict = addr.district || '';
    selectedCommune = addr.commune || '';
    selectedStreetId = addr.streetId;
    selectedStreetName = addr.streetName;
    selectedStreetName1 = addr.streetName1 || '';
    selectedStreetName2 = addr.streetName2 || null;

    // Check if city has streets
    if (addr.streetId === 0) {
        cityHasNoStreets = true;
        document.getElementById('street-input').disabled = true;
        document.getElementById('street-input').classList.add('grayed-out');
    } else {
        cityHasNoStreets = false;
        document.getElementById('street-input').disabled = false;
        document.getElementById('street-input').classList.remove('grayed-out');
    }

    // Scroll to form
    document.getElementById('address-form').scrollIntoView({ behavior: 'smooth' });
};

async function autoSaveSettings() {
    if (!currentSettings) return;
    try {
        return await window.__TAURI__.core.invoke('save_settings', {
            settings: currentSettings
        });
    } catch (error) {
        console.error('Failed to auto-save settings:', error);
    }
}

// ── TERYT Search ──────────────────────────────────────────

async function searchCities(query) {
    try {
        console.log('Searching cities:', query);
        const results = await window.__TAURI__.core.invoke('teryt_lookup_city', { cityName: query });
        console.log('City results:', results);
        renderCitySuggestions(results);
    } catch (error) {
        console.error('City search error:', error);
        const container = document.getElementById('city-suggestions');
        container.innerHTML = `<div class="suggestion-item no-results">Error: ${escapeHtml(String(error))}</div>`;
        container.classList.remove('hidden');
    }
}

function renderCitySuggestions(cities) {
    const container = document.getElementById('city-suggestions');
    if (!cities || cities.length === 0) {
        container.innerHTML = '<div class="suggestion-item no-results">No cities found</div>';
        container.classList.remove('hidden');
        return;
    }

    container.innerHTML = cities.map(c => `
        <div class="suggestion-item" 
            data-city-id="${c.city_id}" 
            data-city-name="${escapeHtml(c.city)}"
            data-voivodeship="${escapeHtml(c.voivodeship)}"
            data-district="${escapeHtml(c.district)}"
            data-commune="${escapeHtml(c.commune)}">
            <div class="suggestion-name">${escapeHtml(c.city)}</div>
            <div class="suggestion-detail">${escapeHtml(c.voivodeship)} / ${escapeHtml(c.district)} / ${escapeHtml(c.commune)}</div>
        </div>
    `).join('');

    container.querySelectorAll('.suggestion-item[data-city-id]').forEach(el => {
        el.addEventListener('click', () => {
            selectedCityId = parseInt(el.dataset.cityId, 10);
            selectedCityName = el.dataset.cityName;
            selectedVoivodeship = el.dataset.voivodeship;
            selectedDistrict = el.dataset.district;
            selectedCommune = el.dataset.commune;
            console.log('City selected:', selectedCityName, 'ID:', selectedCityId, 'Units:', selectedVoivodeship, selectedDistrict, selectedCommune);
            document.getElementById('city-input').value = selectedCityName;
            hideSuggestions('city-suggestions');

            selectedStreetId = null;
            selectedStreetName = '';
            cityHasNoStreets = false;

            // Check if city has streets
            window.__TAURI__.core.invoke('teryt_city_has_streets', { cityId: selectedCityId })
                .then(hasStreets => {
                    cityHasNoStreets = !hasStreets;
                    const streetInput = document.getElementById('street-input');
                    if (cityHasNoStreets) {
                        streetInput.value = t('no_streets');
                        streetInput.disabled = true;
                        streetInput.classList.add('grayed-out');
                        selectedStreetId = 0; // special ID for no streets
                        selectedStreetName = '';
                        selectedStreetName1 = '';
                        selectedStreetName2 = null;
                        document.getElementById('house-input').focus();
                    } else {
                        streetInput.value = '';
                        streetInput.disabled = false;
                        streetInput.classList.remove('grayed-out');
                        streetInput.focus();
                    }
                })
                .catch(err => {
                    console.error('Error checking city streets:', err);
                    document.getElementById('street-input').disabled = false;
                    document.getElementById('street-input').focus();
                });
        });
    });

    container.classList.remove('hidden');
}

async function searchStreets(query) {
    if (!selectedCityId) {
        console.warn('searchStreets: no city selected');
        return;
    }
    try {
        console.log('Searching streets for city_id:', selectedCityId, 'query:', query);
        const results = await window.__TAURI__.core.invoke('teryt_lookup_street', {
            cityId: selectedCityId,
            streetName: query
        });
        console.log('Street results:', results);
        renderStreetSuggestions(results);
    } catch (error) {
        console.error('Street search error:', error);
        const container = document.getElementById('street-suggestions');
        container.innerHTML = `<div class="suggestion-item no-results">Error: ${escapeHtml(String(error))}</div>`;
        container.classList.remove('hidden');
    }
}

function renderStreetSuggestions(streets) {
    const container = document.getElementById('street-suggestions');
    if (!streets || streets.length === 0) {
        container.innerHTML = '<div class="suggestion-item no-results">No streets found</div>';
        container.classList.remove('hidden');
        return;
    }

    container.innerHTML = streets.map(s => `
        <div class="suggestion-item" data-street-id="${s.street_id}" data-street-name="${escapeHtml(s.full_street_name)}" data-street-name1="${escapeHtml(s.street_name_1)}" data-street-name2="${s.street_name_2 ? escapeHtml(s.street_name_2) : ''}">
            <div class="suggestion-name">${escapeHtml(s.full_street_name)}</div>
        </div>
    `).join('');

    container.querySelectorAll('.suggestion-item[data-street-id]').forEach(el => {
        el.addEventListener('click', () => {
            selectedStreetId = parseInt(el.dataset.streetId, 10);
            selectedStreetName = el.dataset.streetName;
            selectedStreetName1 = el.dataset.streetName1;
            selectedStreetName2 = el.dataset.streetName2 || null;
            document.getElementById('street-input').value = selectedStreetName;
            hideSuggestions('street-suggestions');
        });
    });

    container.classList.remove('hidden');
}

function hideSuggestions(id) {
    document.getElementById(id).classList.add('hidden');
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

async function loadSettingsAndFetch() {
    try {
        const settings = await window.__TAURI__.core.invoke('load_settings');
        if (settings) {
            currentSettings = settings;

            if (settings.language && document.getElementById('language-select')) {
                document.getElementById('language-select').value = settings.language;
                initLanguage(settings.language);
            } else {
                initLanguage('system');
            }
            applyTranslations();

            if (settings.theme) {
                document.getElementById('theme-select').value = settings.theme;
            }
            applyTheme(settings.theme || 'system');

            // If enabledSources is present, we respect it as is. 
            // The fallback below handles the case where it's entirely missing.

            const sources = settings.enabledSources || [];
            document.getElementById('source-tauron-check').checked = sources.includes('tauron');
            document.getElementById('source-water-check').checked = sources.includes('water');
            document.getElementById('source-fortum-check').checked = sources.includes('fortum');
            if (document.getElementById('source-energa-check')) {
                document.getElementById('source-energa-check').checked = sources.includes('energa');
            }
            if (document.getElementById('source-enea-check')) {
                document.getElementById('source-enea-check').checked = sources.includes('enea');
            }
            if (document.getElementById('source-pge-check')) {
                document.getElementById('source-pge-check').checked = sources.includes('pge');
            }
            if (document.getElementById('source-stoen-check')) {
                document.getElementById('source-stoen-check').checked = sources.includes('stoen');
            }

            const notifyPrefs = settings.notificationPreferences || {};
            if (document.getElementById('notify-tauron-check')) document.getElementById('notify-tauron-check').checked = !!notifyPrefs.tauron;
            if (document.getElementById('notify-water-check')) document.getElementById('notify-water-check').checked = !!notifyPrefs.water;
            if (document.getElementById('notify-fortum-check')) document.getElementById('notify-fortum-check').checked = !!notifyPrefs.fortum;
            if (document.getElementById('notify-energa-check')) document.getElementById('notify-energa-check').checked = !!notifyPrefs.energa;
            if (document.getElementById('notify-enea-check')) document.getElementById('notify-enea-check').checked = !!notifyPrefs.enea;
            if (document.getElementById('notify-pge-check')) document.getElementById('notify-pge-check').checked = !!notifyPrefs.pge;
            if (document.getElementById('notify-stoen-check')) document.getElementById('notify-stoen-check').checked = !!notifyPrefs.stoen;

            updateAddressFilter();
            renderAddressesList();
            document.getElementById('addresses-list').classList.remove('hidden');
            document.getElementById('add-address-btn').classList.remove('hidden');
            document.getElementById('address-form').classList.add('hidden');

            if (settings.addresses && settings.addresses.length > 0) {
                fetchOutages();
            } else {
                const container = document.getElementById('outages-container');
                container.innerHTML = `<div class="no-outages">${typeof t !== 'undefined' ? t('setup_prompt') : 'Tap ⚙️ to configure your location.'}</div>`;
                document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';
                document.getElementById('settings-panel').classList.remove('hidden');
                applyTheme('system');
            }
        } else {
            initLanguage('system');
            applyTranslations();
            currentSettings = {
                addresses: [],
                primaryAddressIndex: null,
                theme: 'system',
                language: 'system',
                enabledSources: []
            };
            updateAddressFilter();
            renderAddressesList();
            const container = document.getElementById('outages-container');
            container.innerHTML = `<div class="no-outages">${typeof t !== 'undefined' ? t('setup_prompt') : 'Tap ⚙️ to configure your location.'}</div>`;
            document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';
            document.getElementById('settings-panel').classList.remove('hidden');
            applyTheme('system');
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
}

async function saveNewAddress() {
    const name = document.getElementById('address-name-input').value.trim() || 'Address ' + ((currentSettings?.addresses?.length || 0) + 1);
    const streetName = document.getElementById('street-input').value.trim();
    const houseNo = document.getElementById('house-input').value.trim();
    const status = document.getElementById('settings-status');

    if (!selectedCityId || (!selectedStreetId && !cityHasNoStreets)) {
        status.textContent = typeof t !== 'undefined' ? t('err_fields_required') : '⚠️ Please select a city and street from the lists.';
        status.className = 'settings-status error';
        return;
    }
    if (!houseNo) {
        status.textContent = typeof t !== 'undefined' ? t('err_fields_required') : '⚠️ House number is required.';
        status.className = 'settings-status error';
        return;
    }

    const saveBtn = document.getElementById('save-settings-btn');
    saveBtn.disabled = true;

    try {
        const statusMsg = typeof t !== 'undefined' ? t('msg_saving') : '💾 Saving...';
        status.textContent = statusMsg;
        status.className = 'settings-status';
        const address = {
            name,
            cityName: selectedCityName,
            voivodeship: selectedVoivodeship,
            district: selectedDistrict,
            commune: selectedCommune,
            streetName: selectedStreetName,
            streetName1: selectedStreetName1,
            streetName2: selectedStreetName2,
            houseNo,
            cityId: selectedCityId,
            streetId: selectedStreetId
        };

        if (editingAddressIndex !== null) {
            // Update existing address
            currentSettings.addresses[editingAddressIndex] = address;
            await window.__TAURI__.core.invoke('save_settings', { settings: currentSettings });
        } else {
            // Add new address
            currentSettings = await window.__TAURI__.core.invoke('add_address', { address });
        }

        status.textContent = typeof t !== 'undefined' ? t('msg_saved') : '✅ Saved!';
        status.className = 'settings-status success';

        document.getElementById('address-form').classList.add('hidden');
        document.getElementById('add-address-btn').classList.remove('hidden');
        document.getElementById('addresses-list').classList.remove('hidden');

        editingAddressIndex = null;
        updateAddressFilter();
        renderAddressesList();

        setTimeout(() => {
            status.textContent = '';
            if (currentSettings.addresses.length === 1) {
                document.getElementById('settings-panel').classList.add('hidden');
            }
        }, 1500);

        fetchOutages();
    } catch (error) {
        status.textContent = `❌ ${error}`;
        status.className = 'settings-status error';
    } finally {
        saveBtn.disabled = false;
    }
}

function applyTheme(theme) {
    const root = document.documentElement;
    if (!theme || theme === 'system') {
        const sysTheme = (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) ? 'dark' : 'light';
        root.setAttribute('data-theme', sysTheme);
    } else {
        root.setAttribute('data-theme', theme);
    }
}

// Watch for system theme changes
if (window.matchMedia) {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', e => {
        const currentSetting = document.getElementById('theme-select');
        if (currentSetting && currentSetting.value === 'system') {
            applyTheme('system');
        }
    });
}

// ── Pull to Refresh ───────────────────────────────────────

function initRefreshButton() {
    const refreshBtn = document.getElementById('refresh-btn');
    if (!refreshBtn) return;

    refreshBtn.addEventListener('click', async () => {
        if (refreshBtn.classList.contains('spinning')) return;
        refreshBtn.classList.add('spinning');
        await fetchOutages();
        refreshBtn.classList.remove('spinning');
    });
}

function initPullToRefresh() {
    const indicator = document.getElementById('pull-indicator');
    let startY = 0;
    let pulling = false;
    const threshold = 80;

    document.addEventListener('touchstart', (e) => {
        if (window.scrollY === 0) {
            startY = e.touches[0].clientY;
            pulling = true;
        }
    }, { passive: true });

    document.addEventListener('touchmove', (e) => {
        if (!pulling) return;
        const dy = e.touches[0].clientY - startY;
        if (dy > 10 && window.scrollY === 0) {
            indicator.classList.toggle('visible', dy > threshold / 2);
        }
    }, { passive: true });

    document.addEventListener('touchend', () => {
        if (!pulling) return;
        pulling = false;
        if (indicator.classList.contains('visible')) {
            indicator.classList.remove('visible');
            indicator.classList.add('refreshing');
            indicator.textContent = typeof t !== 'undefined' ? t('refresh_loading') : '↻ Refreshing...';
            fetchOutages().finally(() => {
                indicator.classList.remove('refreshing');
                indicator.textContent = typeof t !== 'undefined' ? t('refresh_pull') : '↻ Release to refresh';
            });
        }
    });
}

// ── Alerts ─────────────────────────────────────────────────

async function fetchOutages() {
    const container = document.getElementById('outages-container');
    try {
        const alerts = await window.__TAURI__.core.invoke('fetch_all_alerts');
        lastAlerts = alerts;
        updateLastUpdated(new Date());
        renderAlerts(alerts, container, currentSettings, selectedAddressIndex);
    } catch (error) {
        console.error('Error fetching data:', error);
        container.innerHTML = `<div class="error">${typeof t !== 'undefined' ? t('err_load_failed') : 'Failed to load alert data. Error: '}${error}</div>`;
    }
}

function updateLastUpdated(date) {
    if (date) lastFetchDate = date;
    const el = document.getElementById('last-updated');
    if (!el) return;

    if (!lastFetchDate) {
        el.textContent = typeof t !== 'undefined' ? t('checking_updates') : 'Checking for updates...';
        return;
    }

    // We remove data-i18n so applyTranslations doesn't overwrite our manual timestamp
    el.removeAttribute('data-i18n');

    const localeStr = typeof getLocaleString !== 'undefined' ? getLocaleString() : 'pl-PL';
    const label = typeof t !== 'undefined' ? t('last_updated') : 'Last updated';
    el.textContent = `${label}: ${lastFetchDate.toLocaleTimeString(localeStr)}`;
}

function filterAlerts(alerts, streetName) {
    if (!alerts || !streetName) return [];

    const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const wordMatch = (text, word) => {
        const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(word)}([^\\p{L}]|$)`, 'iu');
        return regex.test(text);
    };

    const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
    const fullStreet = normalize(streetName);
    const significantWords = fullStreet.split(/\s+/).filter(w => w.length >= 3);

    return alerts.filter(item => {
        if (!item.message) return false;
        return significantWords.some(word => wordMatch(item.message, word));
    });
}

// Legacy wrapper — used by old filterOutages tests
function filterOutages(allOutages, streetName, settings) {
    if (!allOutages) return [];

    const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const wordMatch = (text, word) => {
        const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(word)}([^\\p{L}]|$)`, 'iu');
        return regex.test(text);
    };

    const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
    const fullStreet = normalize(streetName);
    const significantWords = fullStreet.split(/\s+/).filter(w => w.length >= 3);

    return allOutages.filter(item => {
        if (!item.Message && !item.message) return false;
        if (!streetName) return false;

        const message = item.Message || item.message || '';
        return significantWords.some(word => wordMatch(message, word));
    });
}

function matchesStreetName(alert, addr) {
    if (!alert.message || !addr) return false;

    const message = alert.message;
    const streetName1 = addr.streetName1 || '';
    const streetName2 = addr.streetName2 || null;

    const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

    if (!streetName1) {
        // Fallback for cities without streets: match by city name in the message
        const cityName = addr.cityName || '';
        if (!cityName) return false;
        const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(cityName)}([^\\p{L}]|$)`, 'iu');
        return regex.test(message);
    }
    const wordMatch = (word) => {
        const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(word)}([^\\p{L}]|$)`, 'iu');
        return regex.test(message);
    };

    // Priority: compound name first (if nazwa_2 exists)
    if (streetName2) {
        const compound = `${streetName2.trim()} ${streetName1.trim()}`;
        if (wordMatch(compound)) return true;
    }

    // Fallback: individual significant words (>= 3 chars)
    const words1 = streetName1.split(/\s+/).filter(w => w.length >= 3);
    if (words1.some(wordMatch)) return true;

    if (streetName2) {
        const words2 = streetName2.split(/\s+/).filter(w => w.length >= 3);
        if (words2.some(wordMatch)) return true;
    }

    return false;
}

function matchesAddress(alert, addresses, addrIdx) {
    const addr = addresses[addrIdx];
    if (!addr) return false;

    if (alert.source === 'tauron' || alert.source === 'energa' || alert.source === 'enea' || alert.source === 'pge' || alert.source === 'stoen') {
        return alert.isLocal === true && alert.addressIndex === addrIdx;
    }

    // For Water and Fortum, use street name matching
    if (!alert.message) return false;
    return matchesStreetName(alert, addr);
}

function renderAlerts(alerts, container, settings, selectedAddrIdx = -1) {
    const now = new Date();

    const enabledSources = (settings && settings.enabledSources) ? settings.enabledSources : ['tauron', 'water', 'fortum', 'energa', 'enea', 'pge', 'stoen'];
    const activeAlerts = alerts.filter(item => {
        if (!enabledSources.includes(item.source)) return false;
        if (!item.endDate) return true;
        const end = new Date(item.endDate);
        return isNaN(end.getTime()) || end > now;
    });

    const addresses = (settings && settings.addresses) ? settings.addresses : [];

    let localTauron = [], otherTauron = [];
    let localWater = [], otherWater = [];
    let localFortum = [], otherFortum = [];
    let localEnerga = [], otherEnerga = [];
    let localEnea = [], otherEnea = [];
    let localPge = [], otherPge = [];
    let localStoen = [], otherStoen = [];

    if (selectedAddrIdx >= 0 && addresses[selectedAddrIdx]) {
        activeAlerts.forEach(item => {
            if (item.source === 'tauron') {
                // Only show alerts from the selected address
                if (item.addressIndex === selectedAddrIdx) {
                    if (item.isLocal === true) {
                        localTauron.push(item);
                    } else {
                        otherTauron.push(item);
                    }
                }
                // Skip alerts from other addresses - don't show them at all when filtered by specific address
            } else if (item.source === 'water') {
                const addr = addresses[selectedAddrIdx];
                if (addr && matchesStreetName(item, addr)) {
                    localWater.push(item);
                } else {
                    otherWater.push(item);
                }
            } else if (item.source === 'fortum') {
                const addr = addresses[selectedAddrIdx];
                if (addr && matchesStreetName(item, addr)) {
                    localFortum.push(item);
                } else {
                    otherFortum.push(item);
                }
            } else if (item.source === 'energa') {
                if (item.addressIndex === selectedAddrIdx && item.isLocal === true) {
                    localEnerga.push(item);
                } else {
                    otherEnerga.push(item);
                }
            } else if (item.source === 'enea') {
                if (item.addressIndex === selectedAddrIdx && item.isLocal === true) {
                    localEnea.push(item);
                } else {
                    otherEnea.push(item);
                }
            } else if (item.source === 'pge') {
                if (item.addressIndex === selectedAddrIdx && item.isLocal === true) {
                    localPge.push(item);
                } else {
                    otherPge.push(item);
                }
            } else if (item.source === 'stoen') {
                if (item.addressIndex === selectedAddrIdx && item.isLocal === true) {
                    localStoen.push(item);
                } else {
                    otherStoen.push(item);
                }
            }
        });
    } else if (addresses.length > 0) {
        activeAlerts.forEach(item => {
            if (item.source === 'tauron') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localTauron.push(item);
                } else {
                    otherTauron.push(item);
                }
            } else if (item.source === 'water') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localWater.push(item);
                } else {
                    otherWater.push(item);
                }
            } else if (item.source === 'fortum') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localFortum.push(item);
                } else {
                    otherFortum.push(item);
                }
            } else if (item.source === 'energa') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localEnerga.push(item);
                } else {
                    otherEnerga.push(item);
                }
            } else if (item.source === 'enea') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localEnea.push(item);
                } else {
                    otherEnea.push(item);
                }
            } else if (item.source === 'pge') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localPge.push(item);
                } else {
                    otherPge.push(item);
                }
            } else if (item.source === 'stoen') {
                const isLocal = addresses.some((_, idx) => matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localStoen.push(item);
                } else {
                    otherStoen.push(item);
                }
            }
        });
    } else {
        otherTauron = activeAlerts.filter(a => a.source === 'tauron');
        otherWater = activeAlerts.filter(a => a.source === 'water');
        otherFortum = activeAlerts.filter(a => a.source === 'fortum');
        otherEnerga = activeAlerts.filter(a => a.source === 'energa');
        otherEnea = activeAlerts.filter(a => a.source === 'enea');
        otherPge = activeAlerts.filter(a => a.source === 'pge');
        otherStoen = activeAlerts.filter(a => a.source === 'stoen');
    }

    const hasLocalAlerts = localTauron.length > 0 || localWater.length > 0 || localFortum.length > 0 || localEnerga.length > 0 || localEnea.length > 0 || localPge.length > 0 || localStoen.length > 0;
    const hasOtherAlerts = otherTauron.length > 0 || otherWater.length > 0 || otherFortum.length > 0 || otherEnerga.length > 0 || otherEnea.length > 0 || otherPge.length > 0 || otherStoen.length > 0;
    const hasAnyAlerts = hasLocalAlerts || hasOtherAlerts;

    container.innerHTML = '';

    if (!hasAnyAlerts) {
        const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
        const msgNoLoc = typeof t !== 'undefined' ? t('msg_no_outages_local') : 'No planned outages for your location.';
        container.innerHTML = `<div class="section-label">${lblYourLoc}</div><div class="no-outages">${msgNoLoc}</div>`;
        return;
    }

    if (hasLocalAlerts) {
        const totalLocal = localTauron.length + localWater.length + localFortum.length + localEnerga.length + localEnea.length + localPge.length + localStoen.length;
        const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
        container.innerHTML += `<div class="section-label">${lblYourLoc} (${totalLocal})</div>`;

        if (localTauron.length > 0) {
            container.innerHTML += renderCards(localTauron, 'tauron');
        }
        if (localWater.length > 0) {
            container.innerHTML += renderCards(localWater, 'water');
        }
        if (localFortum.length > 0) {
            container.innerHTML += renderCards(localFortum, 'fortum');
        }
        if (localEnerga.length > 0) {
            container.innerHTML += renderCards(localEnerga, 'energa');
        }
        if (localEnea.length > 0) {
            container.innerHTML += renderCards(localEnea, 'enea');
        }
        if (localPge.length > 0) {
            container.innerHTML += renderCards(localPge, 'pge');
        }
        if (localStoen.length > 0) {
            container.innerHTML += renderCards(localStoen, 'stoen');
        }
    }

    if (hasOtherAlerts) {
        const lblDivider = typeof t !== 'undefined' ? t('lbl_other_alerts_divider') : 'Other alerts';
        container.innerHTML += `<div class="other-divider"><span>${lblDivider}</span></div>`;

        if (otherTauron.length > 0) {
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_tauron') : 'Power (Tauron)';
            container.innerHTML += `
                <div class="collapsible source-tauron collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherTauron.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherTauron, 'tauron')}
                    </div>
                </div>
            `;
        }

        if (otherWater.length > 0) {
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_water') : 'Water (MPWiK)';
            container.innerHTML += `
                <div class="collapsible source-water collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherWater.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherWater, 'water')}
                    </div>
                </div>
            `;
        }

        if (otherFortum.length > 0) {
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_fortum') : 'Heating (Fortum)';
            container.innerHTML += `
                <div class="collapsible source-fortum collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherFortum.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherFortum, 'fortum')}
                    </div>
                </div>
            `;
        }
        if (otherEnerga.length > 0) {
            const lblSection = (typeof t !== 'undefined' ? t('lbl_section_energa') : null) || 'Power (Energa)';
            container.innerHTML += `
                <div class="collapsible source-energa collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherEnerga.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherEnerga, 'energa')}
                    </div>
                </div>
            `;
        }
        if (otherEnea.length > 0) {
            const lblSection = (typeof t !== 'undefined' ? t('lbl_section_enea') : null) || 'Power (Enea)';
            container.innerHTML += `
                <div class="collapsible source-enea collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherEnea.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherEnea, 'enea')}
                    </div>
                </div>
            `;
        }
        if (otherPge.length > 0) {
            const lblSection = (typeof t !== 'undefined' ? t('lbl_section_pge') : null) || 'Power (PGE)';
            container.innerHTML += `
                <div class="collapsible source-pge collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherPge.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherPge, 'pge')}
                    </div>
                </div>
            `;
        }
        if (otherStoen.length > 0) {
            const lblSection = (typeof t !== 'undefined' ? t('lbl_section_stoen') : null) || 'Power (Stoen)';
            container.innerHTML += `
                <div class="collapsible source-stoen collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${lblSection} (${otherStoen.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherStoen, 'stoen')}
                    </div>
                </div>
            `;
        }
    }
}

function renderCards(alerts, source) {
    const sourceLabel = source === 'water'
        ? ((typeof t !== 'undefined' ? t('source_water') : null) || '💧 Water Outage')
        : source === 'fortum'
            ? ((typeof t !== 'undefined' ? t('source_fortum') : null) || '🔥 Heating Outage (Fortum)')
            : source === 'energa'
                ? ((typeof t !== 'undefined' ? t('source_energa') : null) || '⚡ Energa Outage')
                : source === 'enea'
                    ? ((typeof t !== 'undefined' ? t('source_enea') : null) || '⚡ Enea Outage')
                    : source === 'pge'
                        ? ((typeof t !== 'undefined' ? t('source_pge') : null) || '⚡ PGE Outage')
                        : source === 'stoen'
                            ? ((typeof t !== 'undefined' ? t('source_stoen') : null) || '⚡ Stoen Outage')
                            : ((typeof t !== 'undefined' ? t('source_tauron') : null) || '⚡ Power Outage');

    return alerts.map(item => `
        <div class="card source-${source}">
            <span class="outage-type">${sourceLabel}</span>
            <div class="outage-time">
                ${formatDate(item.startDate)} – ${formatDate(item.endDate)}
            </div>
            ${item.description ? `<div class="outage-reason">${item.description}</div>` : ''}
            ${item.message ? `<div class="outage-message">${item.message}</div>` : ''}
        </div>
    `).join('');
}

function formatDate(dateString) {
    if (!dateString) return '';
    const date = new Date(dateString);
    const localeStr = typeof getLocaleString !== 'undefined' ? getLocaleString() : 'pl-PL';
    return date.toLocaleString(localeStr, {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
        hour: '2-digit',
        minute: '2-digit'
    });
}

// Export for tests
if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        filterOutages,
        filterAlerts,
        formatDate
    };
}

