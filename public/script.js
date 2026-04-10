if (typeof document !== 'undefined') {
    document.addEventListener('DOMContentLoaded', () => {
        initSettings();
        initPullToRefresh();
        initRefreshButton();
        initAddressFilter();
        loadSettingsAndFetch();
        debugSafeAreas();
        fetchAppVersion();
    });

    function debugSafeAreas() {
        if (/Android|iPhone|iPad|iPod/i.test(navigator.userAgent)) {
            const styles = getComputedStyle(document.documentElement);
            const top = styles.getPropertyValue('--safe-area-inset-top').trim();
            const bottom = styles.getPropertyValue('--safe-area-inset-bottom').trim();
            console.log('Mobile Safe Area Insets: ' + JSON.stringify({ top, bottom }));
        }
    }

    async function fetchAppVersion() {
        if (window.__TAURI__) {
            try {
                const version = await window.__TAURI__.core.invoke('get_app_version');
                window.appVersion = version;
                if (typeof applyTranslations === 'function') {
                    applyTranslations();
                }
            } catch (error) {
                console.error('Failed to fetch app version:', error);
            }
        }
    }

    // Handle external links via Tauri opener
    document.addEventListener('click', (e) => {
        const link = e.target.closest('a[target="_blank"]');
        if (link && window.__TAURI__) {
            e.preventDefault();
            console.log('Attempting to open link:', link.href);
            // In Tauri v2, the opener plugin provides an 'open_url' command
            window.__TAURI__.core.invoke('plugin:opener|open_url', { url: link.href })
                .catch(err => {
                    console.error('Failed to open link:', err);
                });
        }
    });
}

// ── Settings ──────────────────────────────────────────────

let currentSettings = null;
let lastAlerts = [];
let lastFetchDate = null;
let selectedAddressIndex = -1; // -1 means "all addresses"
let isFetching = false;
let fetchingSources = new Set();
let isSearchingCities = false;
let isSearchingStreets = false;

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

async function checkAndRequestNotificationPermission() {
    if (!window.__TAURI__) return;
    
    try {
        let granted = await window.__TAURI__.core.invoke('plugin:notification|is_permission_granted');
        
        // On Android, if not granted, try to request it if it's the first time
        // or just if we are trying to enable notifications.
        if (!granted) {
            const permission = await window.__TAURI__.core.invoke('plugin:notification|request_permission');
            granted = (permission === 'granted');
        }
        
        const warning = document.getElementById('notification-permission-warning');
        if (warning) {
            if (granted) {
                warning.classList.add('hidden');
            } else {
                warning.classList.remove('hidden');
            }
        }
    } catch (error) {
        console.error('Failed to check notification permission:', error);
    }
}

function updateUpcomingStatus() {
    const upcomingNotifyCheck = document.getElementById('upcoming-notify-check');
    const upcomingHoursInput = document.getElementById('upcoming-hours-input');
    const adjustContainer = document.getElementById('upcoming-adjust-container');
    const rowContainer = document.getElementById('upcoming-row-container');
    
    const notifyIds = [
        'notify-tauron-check', 'notify-water-check', 'notify-fortum-check', 
        'notify-energa-check', 'notify-enea-check', 'notify-pge-check', 'notify-stoen-check'
    ];
    const anyNotifyChecked = notifyIds.some(id => {
        const cb = document.getElementById(id);
        return cb && cb.checked && !cb.disabled;
    });

    if (upcomingNotifyCheck && adjustContainer && upcomingHoursInput && rowContainer) {
        if (!anyNotifyChecked) {
            rowContainer.classList.add('notify-disabled');
            upcomingNotifyCheck.disabled = true;
            adjustContainer.classList.add('notify-disabled');
            upcomingHoursInput.disabled = true;
        } else {
            rowContainer.classList.remove('notify-disabled');
            upcomingNotifyCheck.disabled = false;
            
            if (upcomingNotifyCheck.checked) {
                adjustContainer.classList.remove('notify-disabled');
                upcomingHoursInput.disabled = false;
            } else {
                adjustContainer.classList.add('notify-disabled');
                upcomingHoursInput.disabled = true;
            }
        }
    }
}

function initSettings() {
    const btn = document.getElementById('settings-btn');
    const panel = document.getElementById('settings-panel');
    const saveBtn = document.getElementById('save-settings-btn');
    const themeSelect = document.getElementById('theme-select');
    const langSelect = document.getElementById('language-select');
    const addAddressBtn = document.getElementById('add-address-btn');
    btn.addEventListener('click', () => {
        panel.classList.toggle('hidden');
        if (!panel.classList.contains('hidden')) {
            window.scrollTo({ top: 0, behavior: 'instant' });
            panel.scrollTop = 0;
            checkAndRequestNotificationPermission(); // Update permission warning state
        }
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
        document.getElementById('city-input').parentElement.classList.remove('valid');
        document.getElementById('street-input').parentElement.classList.remove('valid');
        hideSuggestions('city-suggestions');
        hideSuggestions('street-suggestions');
        
        // Scroll to form
        document.getElementById('address-form').scrollIntoView({ behavior: 'smooth' });
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
        document.getElementById('city-input').parentElement.classList.remove('valid');
        document.getElementById('street-input').parentElement.classList.remove('valid');
        hideSuggestions('city-suggestions');
        hideSuggestions('street-suggestions');
    });

    const cityInput = document.getElementById('city-input');
    cityInput.addEventListener('input', () => {
        // Clear selection if input changes
        if (selectedCityId) {
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
            document.getElementById('street-input').parentElement.classList.remove('valid');
            cityInput.parentElement.classList.remove('valid');
            cityHasNoStreets = false;
            hideSuggestions('street-suggestions');
        }

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
        // Clear selection if input changes
        if (selectedStreetId && !cityHasNoStreets) {
            selectedStreetId = null;
            selectedStreetName = '';
            selectedStreetName1 = '';
            selectedStreetName2 = null;
            streetInput.parentElement.classList.remove('valid');
        }

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


    const sourceNotifyPairs = [
        { source: 'source-tauron-check', notify: 'notify-tauron-check' },
        { source: 'source-water-check', notify: 'notify-water-check' },
        { source: 'source-fortum-check', notify: 'notify-fortum-check' },
        { source: 'source-energa-check', notify: 'notify-energa-check' },
        { source: 'source-enea-check', notify: 'notify-enea-check' },
        { source: 'source-pge-check', notify: 'notify-pge-check' },
        { source: 'source-stoen-check', notify: 'notify-stoen-check' }
    ];

    function updateNotifyStatus(sourceId, notifyId) {
        const sourceCheck = document.getElementById(sourceId);
        const notifyCheck = document.getElementById(notifyId);
        if (sourceCheck && notifyCheck) {
            notifyCheck.disabled = !sourceCheck.checked;
            // Add a class only to the notification group for visual feedback
            const notifyGroup = notifyCheck.closest('.notify-group');
            if (notifyGroup) {
                if (notifyCheck.disabled) {
                    notifyGroup.classList.add('notify-disabled');
                } else {
                    notifyGroup.classList.remove('notify-disabled');
                }
            }
        }
    }

    sourceNotifyPairs.forEach(pair => {
        const sourceCheckbox = document.getElementById(pair.source);
        if (!sourceCheckbox) return;
        sourceCheckbox.addEventListener('change', async () => {
            if (!currentSettings) return;
            const enabledSources = [];
            sourceNotifyPairs.forEach(p => {
                const cb = document.getElementById(p.source);
                if (cb && cb.checked) {
                    const srcName = p.source.split('-')[1]; // tauron, water, etc.
                    enabledSources.push(srcName);
                }
            });
            currentSettings.enabledSources = enabledSources;
            updateNotifyStatus(pair.source, pair.notify);
            updateUpcomingStatus();
            await autoSaveSettings();
            if (sourceCheckbox.checked) {
                const srcName = pair.source.split('-')[1];
                fetchOutages(srcName);
            } else {
                const container = document.getElementById('outages-container');
                renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
            }
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
            
            // Check for any enabled notifications to trigger permission reminder
            if (checkbox.checked) {
                await checkAndRequestNotificationPermission();
            }
            
            updateUpcomingStatus();
            await autoSaveSettings();
        });
    });

    const upcomingNotifyCheck = document.getElementById('upcoming-notify-check');
    const upcomingHoursInput = document.getElementById('upcoming-hours-input');



    if (upcomingNotifyCheck) {
        upcomingNotifyCheck.addEventListener('change', async () => {
            updateUpcomingStatus();
            if (upcomingNotifyCheck.checked) {
                await checkAndRequestNotificationPermission();
            }
            if (currentSettings) {
                currentSettings.upcomingNotificationEnabled = upcomingNotifyCheck.checked;
                await autoSaveSettings();
            }
        });
    }

    if (upcomingHoursInput) {
        upcomingHoursInput.addEventListener('change', async () => {
            if (currentSettings) {
                let val = parseInt(upcomingHoursInput.value, 10);
                if (isNaN(val) || val < 1) val = 24;
                currentSettings.upcomingNotificationHours = val;
                await autoSaveSettings();
            }
        });
    }
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
    if (allOpt) filter.appendChild(allOpt);

    const activeAddresses = (currentSettings && currentSettings.addresses) 
        ? currentSettings.addresses.map((addr, idx) => ({ ...addr, originalIndex: idx })).filter(addr => addr.isActive !== false)
        : [];

    const activeCount = activeAddresses.length;

    if (activeCount === 0) {
        filter.classList.add('hidden');
    } else if (activeCount === 1) {
        filter.classList.add('hidden');
        selectedAddressIndex = activeAddresses[0].originalIndex;
    } else {
        filter.classList.remove('hidden');
        if (wasHidden) {
            selectedAddressIndex = -1;
            filter.value = '-1';
        }
        activeAddresses.forEach((addr) => {
            const opt = document.createElement('option');
            opt.value = addr.originalIndex;
            opt.textContent = addr.name || `${addr.streetName} ${addr.houseNo}`;
            if (addr.originalIndex === currentSettings.primaryAddressIndex) {
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
        <div class="address-item ${addr.isActive === false ? 'disabled' : ''}">
            <div class="checkbox-pair mini" style="margin-right: 0.75rem; margin-top: 2px;">
                <input type="checkbox" ${addr.isActive !== false ? 'checked' : ''} onchange="toggleAddressActive(${idx})" title="${addr.isActive === false ? (typeof t !== 'undefined' ? t('lbl_address_disabled') : 'Disabled') : (typeof t !== 'undefined' ? t('lbl_address_active') : 'Active')}">
            </div>
            <div class="address-info">
                <div class="address-name">${addr.name || (typeof t !== 'undefined' ? t('address_name') + ' ' + (idx + 1) : 'Address ' + (idx + 1))}</div>
                <div class="address-detail">${addr.streetName} ${addr.houseNo}, ${addr.cityName}</div>
            </div>
            <div class="address-actions">
                ${idx === currentSettings.primaryAddressIndex ? '<span class="primary-badge" title="Primary">⭐</span>' : `<button class="icon-btn" onclick="setPrimaryAddress(${idx})" title="Set as primary">⭐</button>`}
                <button class="icon-btn edit-btn" onclick="editAddress(${idx})" title="Edit">✏️</button>
                <button class="icon-btn delete-btn" onclick="removeAddress(${idx})" title="Remove">🗑️</button>
            </div>
        </div>
    `).join('');
}

window.toggleAddressActive = async function (idx) {
    if (!currentSettings || !currentSettings.addresses[idx]) return;
    const addr = currentSettings.addresses[idx];
    addr.isActive = !addr.isActive;

    try {
        await window.__TAURI__.core.invoke('save_settings', { settings: currentSettings });
        renderAddressesList();
        updateAddressFilter();
        fetchOutages();
    } catch (error) {
        console.error('Error toggling address status:', error);
        addr.isActive = !addr.isActive; // revert on error
        renderAddressesList();
    }
};

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

    if (selectedCityId) {
        document.getElementById('city-input').parentElement.classList.add('valid');
    }
    if (selectedStreetId !== null) {
        document.getElementById('street-input').parentElement.classList.add('valid');
    }

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
    if (isSearchingCities) return;
    isSearchingCities = true;
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
    } finally {
        isSearchingCities = false;
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
            const cityData = {
                city_id: parseInt(el.dataset.cityId, 10),
                city: el.dataset.cityName,
                voivodeship: el.dataset.voivodeship,
                district: el.dataset.district,
                commune: el.dataset.commune
            };
            selectCity(cityData);
        });
    });

    container.classList.remove('hidden');

    const cityQueryValue = document.getElementById('city-input').value.trim().toLowerCase();
    const exactMatches = cities.filter(c => c.city.toLowerCase() === cityQueryValue);
    
    // Only auto-select if there is exactly ONE exact name match.
    // If there are multiple cities with the same name, the user must choose manually.
    if (exactMatches.length === 1 && !selectedCityId) {
        selectCity(exactMatches[0]);
    }
}

function selectCity(c) {
    selectedCityId = c.city_id;
    selectedCityName = c.city;
    selectedVoivodeship = c.voivodeship;
    selectedDistrict = c.district;
    selectedCommune = c.commune;
    
    const cityInput = document.getElementById('city-input');
    cityInput.value = selectedCityName;
    cityInput.parentElement.classList.add('valid');
    cityInput.parentElement.classList.remove('invalid');
    hideSuggestions('city-suggestions');

    selectedStreetId = null;
    selectedStreetName = '';
    cityHasNoStreets = false;
    document.getElementById('street-input').parentElement.classList.remove('valid');

    // Check if city has streets
    window.__TAURI__.core.invoke('teryt_city_has_streets', { cityId: selectedCityId })
        .then(hasStreets => {
            cityHasNoStreets = !hasStreets;
            const streetInput = document.getElementById('street-input');
            if (cityHasNoStreets) {
                streetInput.value = typeof t !== 'undefined' ? t('no_streets') : 'No streets';
                streetInput.disabled = true;
                streetInput.classList.add('grayed-out');
                selectedStreetId = 0; // special ID for no streets
                selectedStreetName = '';
                selectedStreetName1 = '';
                selectedStreetName2 = null;
                streetInput.parentElement.classList.add('valid');
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
}

async function searchStreets(query) {
    if (!selectedCityId || isSearchingStreets) {
        if (!selectedCityId) console.warn('searchStreets: no city selected');
        return;
    }
    isSearchingStreets = true;
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
    } finally {
        isSearchingStreets = false;
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
            const streetData = {
                street_id: parseInt(el.dataset.streetId, 10),
                full_street_name: el.dataset.streetName,
                street_name_1: el.dataset.streetName1,
                street_name_2: el.dataset.streetName2 || null
            };
            selectStreet(streetData);
        });
    });

    container.classList.remove('hidden');

    const streetQueryValue = document.getElementById('street-input').value.trim().toLowerCase();
    const exactMatches = streets.filter(s => s.full_street_name.toLowerCase() === streetQueryValue);

    // Only auto-select if there is exactly ONE exact name match.
    if (exactMatches.length === 1 && !selectedStreetId) {
        selectStreet(exactMatches[0]);
    }
}

function selectStreet(s) {
    selectedStreetId = s.street_id;
    selectedStreetName = s.full_street_name;
    selectedStreetName1 = s.street_name_1;
    selectedStreetName2 = s.street_name_2;
    
    const streetInput = document.getElementById('street-input');
    streetInput.value = selectedStreetName;
    streetInput.parentElement.classList.add('valid');
    streetInput.parentElement.classList.remove('invalid');
    hideSuggestions('street-suggestions');
    document.getElementById('house-input').focus();
}

function hideSuggestions(id) {
    document.getElementById(id).classList.add('hidden');
}

function escapeHtml(str) {
    if (typeof str !== 'string') return str;
    return str.replace(/[&<>"']/g, m => ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
    })[m]);
}

async function loadSettingsAndFetch() {
    try {
        const container = document.getElementById('outages-container');
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

            // Update disabled status for all notify checkboxes
            const pairs = [
                { source: 'source-tauron-check', notify: 'notify-tauron-check' },
                { source: 'source-water-check', notify: 'notify-water-check' },
                { source: 'source-fortum-check', notify: 'notify-fortum-check' },
                { source: 'source-energa-check', notify: 'notify-energa-check' },
                { source: 'source-enea-check', notify: 'notify-enea-check' },
                { source: 'source-pge-check', notify: 'notify-pge-check' },
                { source: 'source-stoen-check', notify: 'notify-stoen-check' }
            ];
            pairs.forEach(p => {
                const sourceCheck = document.getElementById(p.source);
                const notifyCheck = document.getElementById(p.notify);
                if (sourceCheck && notifyCheck) {
                    notifyCheck.disabled = !sourceCheck.checked;
                    const notifyGroup = notifyCheck.closest('.notify-group');
                    if (notifyGroup) {
                        if (notifyCheck.disabled) notifyGroup.classList.add('notify-disabled');
                        else notifyGroup.classList.remove('notify-disabled');
                    }
                }
            });

            if (document.getElementById('upcoming-notify-check')) {
                document.getElementById('upcoming-notify-check').checked = !!settings.upcomingNotificationEnabled;
            }
            if (document.getElementById('upcoming-hours-input')) {
                document.getElementById('upcoming-hours-input').value = settings.upcomingNotificationHours !== undefined ? settings.upcomingNotificationHours : 24;
            }
            
            if (typeof updateUpcomingStatus === 'function') {
                updateUpcomingStatus();
            }

            // Also check permissions on load if notifications are enabled
            const hasAnyNotify = Object.values(notifyPrefs).some(v => v === true) || !!settings.upcomingNotificationEnabled;
            if (hasAnyNotify) {
                checkAndRequestNotificationPermission();
            }

            updateAddressFilter();
            renderAddressesList();
            document.getElementById('addresses-list').classList.remove('hidden');
            document.getElementById('add-address-btn').classList.remove('hidden');
            document.getElementById('address-form').classList.add('hidden');

            if (settings.addresses && settings.addresses.length > 0) {
                fetchOutages();
            } else {
                renderAlerts([], container, currentSettings, selectedAddressIndex);
                document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';
                document.getElementById('settings-panel').classList.remove('hidden');
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

            // Explicitly uncheck and disable all source/notify pairs on first run
            const pairs = [
                { source: 'source-tauron-check', notify: 'notify-tauron-check' },
                { source: 'source-water-check', notify: 'notify-water-check' },
                { source: 'source-fortum-check', notify: 'notify-fortum-check' },
                { source: 'source-energa-check', notify: 'notify-energa-check' },
                { source: 'source-enea-check', notify: 'notify-enea-check' },
                { source: 'source-pge-check', notify: 'notify-pge-check' },
                { source: 'source-stoen-check', notify: 'notify-stoen-check' }
            ];
            pairs.forEach(p => {
                const s = document.getElementById(p.source);
                const n = document.getElementById(p.notify);
                if (s) s.checked = false;
                if (n) {
                    n.checked = false;
                    n.disabled = true;
                    const notifyGroup = n.closest('.notify-group');
                    if (notifyGroup) notifyGroup.classList.add('notify-disabled');
                }
            });

            updateAddressFilter();
            renderAddressesList();
            renderAlerts([], container, currentSettings, selectedAddressIndex);
            document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';
            document.getElementById('settings-panel').classList.remove('hidden');
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
}

async function saveNewAddress() {
    const name = document.getElementById('address-name-input').value.trim() || 'Address ' + ((currentSettings?.addresses?.length || 0) + 1);
    const streetName = document.getElementById('street-input').value.trim();
    const houseNo = document.getElementById('house-input').value.trim() || '1';
    const status = document.getElementById('settings-status');

    const cityField = document.getElementById('city-input').parentElement;
    const streetField = document.getElementById('street-input').parentElement;

    if (!selectedCityId || (!selectedStreetId && !cityHasNoStreets)) {
        if (!selectedCityId) cityField.classList.add('invalid');
        if (!selectedStreetId && !cityHasNoStreets) streetField.classList.add('invalid');

        status.textContent = typeof t !== 'undefined' ? t('err_fields_required') : '⚠️ Please select a city and street from the lists.';
        status.className = 'settings-status error';
        
        // Remove invalid class after animation
        setTimeout(() => {
            cityField.classList.remove('invalid');
            streetField.classList.remove('invalid');
        }, 1000);
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
            streetId: selectedStreetId,
            isActive: editingAddressIndex !== null ? (currentSettings.addresses[editingAddressIndex].isActive !== false) : true
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

async function fetchOutages(specificSource = null) {
    if (specificSource) {
        if (fetchingSources.has(specificSource)) return;
        fetchingSources.add(specificSource);
    } else {
        if (isFetching) return;
        isFetching = true;
    }

    const container = document.getElementById('outages-container');
    try {
        const invokeArgs = specificSource ? { sources: [specificSource] } : { sources: null };
        const newAlerts = await window.__TAURI__.core.invoke('fetch_all_alerts', invokeArgs);

        if (specificSource) {
            // Merge new alerts for this source into lastAlerts
            lastAlerts = (lastAlerts || []).filter(a => a.source !== specificSource).concat(newAlerts);
        } else {
            lastAlerts = newAlerts;
        }

        updateLastUpdated(new Date());
        renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
    } catch (error) {
        console.error('Error fetching data:', error);
        // Only show full error message on full fetch
        if (!specificSource) {
            container.innerHTML = `<div class="error">${typeof t !== 'undefined' ? t('err_load_failed') : 'Failed to load alert data. Error: '}${error}</div>`;
        }
    } finally {
        if (specificSource) {
            fetchingSources.delete(specificSource);
        } else {
            isFetching = false;
        }
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

    // Secondary: match main streetName1 as a whole word
    // (e.g. "Kościuszki" if address is "Tadeusza Kościuszki")
    if (wordMatch(streetName1)) return true;

    return false;

    return false;
}

function matchesAddress(alert, addresses, addrIdx) {
    const addr = addresses[addrIdx];
    if (!addr || addr.isActive === false) return false;

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
    const hasAnyActiveAddress = addresses.some(a => a.isActive !== false);

    if (addresses.length === 0) {
        const title = typeof t !== 'undefined' ? t('empty_state_title') : 'Welcome to Awaria';
        const subtitle = typeof t !== 'undefined' ? t('empty_state_subtitle') : 'Start by adding your first location to monitor for power, water, and heat outages.';
        const cta = typeof t !== 'undefined' ? t('empty_state_cta') : 'Add Address';

        container.innerHTML = `
            <div class="empty-state-view">
                <div class="empty-state-icon">📍</div>
                <div class="empty-state-title">${escapeHtml(title)}</div>
                <div class="empty-state-subtitle">${escapeHtml(subtitle)}</div>
                <div class="empty-state-cta-container">
                    <button class="empty-state-cta" id="btn-empty-state-cta">
                        ${escapeHtml(cta)}
                    </button>
                </div>
            </div>
        `;

        const ctaBtn = document.getElementById('btn-empty-state-cta');
        if (ctaBtn) {
            ctaBtn.addEventListener('click', () => {
                const panel = document.getElementById('settings-panel');
                panel.classList.remove('hidden');
                const addBtn = document.getElementById('add-address-btn');
                if (addBtn) addBtn.click();
            });
        }
        return;
    } else if (!hasAnyActiveAddress) {
        const title = typeof t !== 'undefined' ? t('disabled_state_title') : 'Monitoring Paused';
        const subtitle = typeof t !== 'undefined' ? t('disabled_state_subtitle') : 'All your saved locations are currently disabled. Enable them in settings to see outages.';
        const cta = typeof t !== 'undefined' ? t('disabled_state_cta') : 'Open Settings';

        container.innerHTML = `
            <div class="empty-state-view">
                <div class="empty-state-icon">⏸️</div>
                <div class="empty-state-title">${escapeHtml(title)}</div>
                <div class="empty-state-subtitle">${escapeHtml(subtitle)}</div>
                <div class="empty-state-cta-container">
                    <button class="empty-state-cta" id="btn-disabled-state-cta">
                        ${escapeHtml(cta)}
                    </button>
                </div>
            </div>
        `;

        const ctaBtn = document.getElementById('btn-disabled-state-cta');
        if (ctaBtn) {
            ctaBtn.addEventListener('click', () => {
                const panel = document.getElementById('settings-panel');
                panel.classList.remove('hidden');
                const section = document.getElementById('location-settings-section');
                if (section) {
                    section.scrollIntoView({ behavior: 'smooth', block: 'start' });
                }
            });
        }
        return;
    } else if (enabledSources.length === 0) {
        const title = typeof t !== 'undefined' ? t('sources_disabled_state_title') : 'Alerts Disabled';
        const subtitle = typeof t !== 'undefined' ? t('sources_disabled_state_subtitle') : 'No alert sources are enabled. Enable them in settings to see outages.';
        const cta = typeof t !== 'undefined' ? t('disabled_state_cta') : 'Open Settings';

        container.innerHTML = `
            <div class="empty-state-view">
                <div class="empty-state-icon">🔕</div>
                <div class="empty-state-title">${escapeHtml(title)}</div>
                <div class="empty-state-subtitle">${escapeHtml(subtitle)}</div>
                <div class="empty-state-cta-container">
                    <button class="empty-state-cta" id="btn-sources-disabled-cta">
                        ${escapeHtml(cta)}
                    </button>
                </div>
            </div>
        `;

        const ctaBtn = document.getElementById('btn-sources-disabled-cta');
        if (ctaBtn) {
            ctaBtn.addEventListener('click', () => {
                const panel = document.getElementById('settings-panel');
                panel.classList.remove('hidden');
                // Target the "Alert Sources" title
                const sourcesTitle = [...panel.querySelectorAll('.settings-title')].find(el => el.getAttribute('data-i18n') === 'settings_sources');
                if (sourcesTitle) {
                    sourcesTitle.scrollIntoView({ behavior: 'smooth', block: 'start' });
                } else {
                    panel.scrollTo({ top: 0, behavior: 'smooth' });
                }
            });
        }
        return;
    }

    const isWarszawa = (addr) => {
        if (!addr) return false;
        const city = (addr.cityName || '').toLowerCase();
        return city === 'warszawa' || city === 'warsaw' || addr.cityId === 918123;
    };
    const isWroclaw = (addr) => {
        if (!addr) return false;
        const city = (addr.cityName || '').toLowerCase();
        return city === 'wrocław' || city === 'wroclaw' || addr.cityId === 969400;
    };

    const hasAnyWarszawa = addresses.some(isWarszawa);
    const hasAnyWroclaw = addresses.some(isWroclaw);

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
                } else if (isWroclaw(addr)) {
                    otherWater.push(item);
                }
            } else if (item.source === 'fortum') {
                if (item.addressIndex === selectedAddrIdx) {
                    const addr = addresses[selectedAddrIdx];
                    if (addr && matchesStreetName(item, addr)) {
                        localFortum.push(item);
                    } else {
                        otherFortum.push(item);
                    }
                }
            } else if (item.source === 'energa') {
                if (item.addressIndex === selectedAddrIdx) {
                    if (item.isLocal === true) {
                        localEnerga.push(item);
                    } else {
                        otherEnerga.push(item);
                    }
                }
            } else if (item.source === 'enea') {
                if (item.addressIndex === selectedAddrIdx) {
                    if (item.isLocal === true) {
                        localEnea.push(item);
                    } else {
                        otherEnea.push(item);
                    }
                }
            } else if (item.source === 'pge') {
                if (item.addressIndex === selectedAddrIdx) {
                    if (item.isLocal === true) {
                        localPge.push(item);
                    } else {
                        otherPge.push(item);
                    }
                }
            } else if (item.source === 'stoen') {
                const addr = addresses[selectedAddrIdx];
                if (item.addressIndex === selectedAddrIdx && item.isLocal === true) {
                    localStoen.push(item);
                } else if (isWarszawa(addr)) {
                    otherStoen.push(item);
                }
            }
        });
    } else if (addresses.length > 0) {
        activeAlerts.forEach(item => {
            if (item.source === 'tauron') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localTauron.push(item);
                } else {
                    otherTauron.push(item);
                }
            } else if (item.source === 'water') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localWater.push(item);
                } else if (hasAnyWroclaw) {
                    otherWater.push(item);
                }
            } else if (item.source === 'fortum') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localFortum.push(item);
                } else {
                    otherFortum.push(item);
                }
            } else if (item.source === 'energa') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localEnerga.push(item);
                } else {
                    otherEnerga.push(item);
                }
            } else if (item.source === 'enea') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localEnea.push(item);
                } else {
                    otherEnea.push(item);
                }
            } else if (item.source === 'pge') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localPge.push(item);
                } else {
                    otherPge.push(item);
                }
            } else if (item.source === 'stoen') {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localStoen.push(item);
                } else if (hasAnyWarszawa) {
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

    let html = '';
    if (!hasAnyAlerts) {
        const title = typeof t !== 'undefined' ? t('all_clear_title') : 'Everything looks good!';
        const subtitle = typeof t !== 'undefined' ? t('all_clear_subtitle') : 'No outages detected.';
        const providersLbl = typeof t !== 'undefined' ? t('monitored_providers') : 'Monitored Providers';
        const operationalLbl = typeof t !== 'undefined' ? t('status_operational') : 'Operational';
        const refreshLbl = typeof t !== 'undefined' ? t('refresh_now') : 'Refresh Now';

        const statusItems = enabledSources.map(src => {
            const name = typeof t !== 'undefined' ? t(`source_${src}_short`) : src;
            return `
                <div class="status-item">
                    <div class="status-dot"></div>
                    <div class="status-info">
                        <span class="status-name">${escapeHtml(name)}</span>
                        <span class="status-label">${escapeHtml(operationalLbl)}</span>
                    </div>
                </div>
            `;
        }).join('');

        container.innerHTML = `
            <div class="all-clear-view">
                <div class="all-clear-title">${escapeHtml(title)}</div>
                <div class="all-clear-subtitle">${escapeHtml(subtitle)}</div>
                
                <div class="section-label" style="width: 100%; max-width: 450px; margin-bottom: 1rem; text-align: left;">
                    ${escapeHtml(providersLbl)}
                </div>
                <div class="status-dashboard">
                    ${statusItems}
                </div>

                <button class="big-refresh-btn" onclick="fetchOutages()" id="btn-dashboard-refresh">
                    ${escapeHtml(refreshLbl)}
                </button>
            </div>
        `;
        return;
    }

    if (hasLocalAlerts) {
        const totalLocal = localTauron.length + localWater.length + localFortum.length + localEnerga.length + localEnea.length + localPge.length + localStoen.length;
        const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
        html += `<div class="section-label">${escapeHtml(lblYourLoc)} (${totalLocal})</div>`;

        // Order: Power (Tauron, Energa, Enea, PGE, Stoen) -> Heat (Fortum) -> Water (MPWiK)
        if (localTauron.length > 0) {
            html += renderCards(localTauron, 'tauron');
        }
        if (localEnerga.length > 0) {
            html += renderCards(localEnerga, 'energa');
        }
        if (localEnea.length > 0) {
            html += renderCards(localEnea, 'enea');
        }
        if (localPge.length > 0) {
            html += renderCards(localPge, 'pge');
        }
        if (localStoen.length > 0) {
            html += renderCards(localStoen, 'stoen');
        }
        if (localFortum.length > 0) {
            html += renderCards(localFortum, 'fortum');
        }
        if (localWater.length > 0) {
            html += renderCards(localWater, 'water');
        }
    }

    if (hasOtherAlerts) {
        const lblDivider = typeof t !== 'undefined' ? t('lbl_other_alerts_divider') : 'Other alerts';
        html += `<div class="other-divider"><span>${escapeHtml(lblDivider)}</span></div>`;

        // Order: Power (Tauron, Energa, Enea, PGE, Stoen) -> Heat (Fortum) -> Water (MPWiK)
        if (otherTauron.length > 0) {
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_tauron') : 'Power (Tauron)';
            html += `
                <div class="collapsible source-tauron collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherTauron.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherTauron, 'tauron')}
                    </div>
                </div>
            `;
        }

        if (otherEnerga.length > 0) {
            const lblSection = (typeof t !== 'undefined' ? t('lbl_section_energa') : null) || 'Power (Energa)';
            html += `
                <div class="collapsible source-energa collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherEnerga.length})</span>
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
            html += `
                <div class="collapsible source-enea collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherEnea.length})</span>
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
            html += `
                <div class="collapsible source-pge collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherPge.length})</span>
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
            html += `
                <div class="collapsible source-stoen collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherStoen.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherStoen, 'stoen')}
                    </div>
                </div>
            `;
        }

        if (otherFortum.length > 0) {
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_fortum') : 'Heat (Fortum)';
            html += `
                <div class="collapsible source-fortum collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherFortum.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherFortum, 'fortum')}
                    </div>
                </div>
            `;
        }

        if (otherWater.length > 0) {
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_water') : 'Water (MPWiK)';
            html += `
                <div class="collapsible source-water collapsed">
                    <div class="section-label other" onclick="this.parentElement.classList.toggle('collapsed')">
                        <span>${escapeHtml(lblSection)} (${otherWater.length})</span>
                        <span class="toggle-icon">▼</span>
                    </div>
                    <div class="collapsible-content">
                        ${renderCards(otherWater, 'water')}
                    </div>
                </div>
            `;
        }
    }
    container.innerHTML = html;
}

function renderCards(alerts, source) {
    const sourceLabel = source === 'water'
        ? ((typeof t !== 'undefined' ? t('source_water') : null) || '💧 Water Outage')
        : source === 'fortum'
            ? ((typeof t !== 'undefined' ? t('source_fortum') : null) || '🔥 Heat Outage (Fortum)')
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
        <div class="card source-${source}" ${item.hash ? `data-hash="${item.hash}"` : ''}>
            <span class="outage-type">${escapeHtml(sourceLabel)}</span>
            <div class="outage-time">
                ${formatDate(item.startDate)} – ${formatDate(item.endDate)}
            </div>
            ${item.description ? `<div class="outage-reason">${escapeHtml(item.description)}</div>` : ''}
            ${item.message ? `<div class="outage-message">${escapeHtml(item.message)}</div>` : ''}
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

// Listen for notification actions
if (window.__TAURI__) {
    const { listen } = window.__TAURI__.event;

    listen('tauri://notification-action', (event) => {
        console.log('Notification action received:', event);
        const hash = event.payload.notification.extra?.hash;
        if (hash) {
            highlightAlert(hash);
        }
    });
}

async function highlightAlert(hash) {
    console.log('Highlighting alert with hash:', hash);
    
    // Ensure data is loaded
    if (!lastAlerts || lastAlerts.length === 0) {
        await fetchOutages();
    }

    // Give UI time to render
    setTimeout(() => {
        const element = document.querySelector(`.card[data-hash="${hash}"]`);
        if (element) {
            // Expand parent if it's a collapsible
            let parent = element.closest('.collapsible');
            if (parent) {
                parent.classList.remove('collapsed');
            }

            // Scroll into view
            element.scrollIntoView({ behavior: 'smooth', block: 'center' });

            // Highlight effect
            element.classList.add('highlight-pulse');
            setTimeout(() => {
                element.classList.remove('highlight-pulse');
            }, 3000);
        } else {
            console.warn('Alert element not found for hash:', hash);
        }
    }, 500);
}

