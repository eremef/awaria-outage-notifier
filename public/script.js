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
        document.getElementById('house-input').value = '';
        document.getElementById('settings-status').textContent = '';
    });

    document.getElementById('cancel-address-btn').addEventListener('click', function() {
        document.getElementById('address-form').classList.add('hidden');
        document.getElementById('add-address-btn').classList.remove('hidden');
        document.getElementById('addresses-list').classList.remove('hidden');
        document.getElementById('address-name-input').value = '';
        document.getElementById('city-input').value = '';
        document.getElementById('street-input').value = '';
        document.getElementById('house-input').value = '';
        document.getElementById('settings-status').textContent = '';
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
                enabledSources: ['tauron', 'water', 'fortum']
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
                enabledSources: ['tauron', 'water', 'fortum']
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

    ['source-tauron-check', 'source-water-check', 'source-fortum-check'].forEach(id => {
        const checkbox = document.getElementById(id);
        checkbox.addEventListener('change', () => {
            if (!currentSettings) return;
            const enabledSources = [];
            if (document.getElementById('source-tauron-check').checked) enabledSources.push('tauron');
            if (document.getElementById('source-water-check').checked) enabledSources.push('water');
            if (document.getElementById('source-fortum-check').checked) enabledSources.push('fortum');
            currentSettings.enabledSources = enabledSources;
            autoSaveSettings().then(() => {
                const container = document.getElementById('outages-container');
                renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
                updateLastUpdated();
            });
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
    filter.innerHTML = '';
    filter.appendChild(allOpt);
    
    const addressCount = currentSettings && currentSettings.addresses ? currentSettings.addresses.length : 0;
    
    if (addressCount === 0) {
        filter.classList.add('hidden');
    } else if (addressCount === 1) {
        filter.classList.add('hidden');
        selectedAddressIndex = 0;
        const container = document.getElementById('outages-container');
        renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
    } else {
        filter.classList.remove('hidden');
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
                <button class="icon-btn delete-btn" onclick="removeAddress(${idx})" title="Remove">🗑️</button>
            </div>
        </div>
    `).join('');
}

window.setPrimaryAddress = async function(idx) {
    try {
        currentSettings = await window.__TAURI__.core.invoke('set_primary_address', { index: idx });
        renderAddressesList();
        updateAddressFilter();
    } catch (error) {
        console.error('Error setting primary address:', error);
    }
};

window.removeAddress = async function(idx) {
    try {
        currentSettings = await window.__TAURI__.core.invoke('remove_address', { index: idx });
        renderAddressesList();
        updateAddressFilter();
        fetchOutages();
    } catch (error) {
        console.error('Error removing address:', error);
    }
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

            const sources = settings.enabledSources || ['tauron', 'water', 'fortum'];
            document.getElementById('source-tauron-check').checked = sources.includes('tauron');
            document.getElementById('source-water-check').checked = sources.includes('water');
            document.getElementById('source-fortum-check').checked = sources.includes('fortum');

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
                enabledSources: ['tauron', 'water', 'fortum']
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
    const cityName = document.getElementById('city-input').value.trim();
    const streetName = document.getElementById('street-input').value.trim();
    const houseNo = document.getElementById('house-input').value.trim();
    const status = document.getElementById('settings-status');

    if (!cityName || !streetName || !houseNo) {
        status.textContent = typeof t !== 'undefined' ? t('err_fields_required') : '⚠️ All fields are required.';
        status.className = 'settings-status error';
        return;
    }

    const saveBtn = document.getElementById('save-settings-btn');
    saveBtn.disabled = true;

    try {
        status.textContent = typeof t !== 'undefined' ? t('msg_looking_city') : '🔍 Looking up city...';
        status.className = 'settings-status';
        const cities = await window.__TAURI__.core.invoke('lookup_city', { cityName });

        const city = cities.find(c => c.Name === cityName);
        if (!city) {
            const available = cities.map(c => c.Name).join(', ');
            status.textContent = (typeof t !== 'undefined' ? t('err_city_not_found') : `❌ City not found. Did you mean: `) + `${available || 'none'}`;
            status.className = 'settings-status error';
            saveBtn.disabled = false;
            return;
        }

        status.textContent = typeof t !== 'undefined' ? t('msg_looking_street') : '🔍 Looking up street...';
        const streets = await window.__TAURI__.core.invoke('lookup_street', {
            streetName,
            cityGaid: city.GAID
        });

        const street = streets.find(s => s.Name === streetName);
        if (!street) {
            const available = streets.map(s => s.Name).join(', ');
            status.textContent = (typeof t !== 'undefined' ? t('err_street_not_found') : `❌ Street not found. Did you mean: `) + `${available || 'none'}`;
            status.className = 'settings-status error';
            saveBtn.disabled = false;
            return;
        }

        status.textContent = typeof t !== 'undefined' ? t('msg_saving') : '💾 Saving...';

        const address = {
            name,
            cityName,
            streetName,
            houseNo,
            cityGAID: city.GAID,
            streetGAID: street.GAID
        };

        currentSettings = await window.__TAURI__.core.invoke('add_address', { address });

        status.textContent = typeof t !== 'undefined' ? t('msg_saved') : '✅ Saved!';
        status.className = 'settings-status success';

        document.getElementById('address-form').classList.add('hidden');
        document.getElementById('add-address-btn').classList.remove('hidden');
        document.getElementById('addresses-list').classList.remove('hidden');
        
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

    const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
    const fullStreet = normalize(streetName);

    if (!fullStreet) return [];

    const escapeRegExp = (string) => string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const significantWords = fullStreet.split(/\s+/).filter(word => word.length >= 3);

    return alerts.filter(item => {
        if (!item.message) return false;
        const message = item.message;

        if (message.includes(streetName)) return true;

        return significantWords.some(word => {
            const regex = new RegExp(`\\b${escapeRegExp(word)}\\b`);
            return regex.test(message);
        });
    });
}

// Legacy wrapper — used by old filterOutages tests
function filterOutages(allOutages, streetName, settings) {
    if (!allOutages) return [];

    const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
    const fullStreet = normalize(streetName);

    if (!fullStreet) return [];

    const escapeRegExp = (string) => string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const significantWords = fullStreet.split(/\s+/).filter(word => word.length >= 3);

    return allOutages.filter(item => {
        if (settings && settings.streetGAID && item.GAID === settings.streetGAID) {
            return true;
        }

        if (!item.Message && !item.message) return false;
        if (!streetName) return false;

        const message = item.Message || item.message || '';

        if (message.includes(streetName)) return true;

        return significantWords.some(word => {
            const regex = new RegExp(`\\b${escapeRegExp(word)}\\b`);
            return regex.test(message);
        });
    });
}

function matchesStreetName(alert, streetName) {
    if (!alert.message || !streetName) return false;
    
    if (alert.message.includes(streetName)) return true;
    
    const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
    const fullStreet = normalize(streetName);
    const significantWords = fullStreet.split(/\s+/).filter(w => w.length >= 3);
    const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    
    return significantWords.some(word => {
        const regex = new RegExp(`\\b${escapeRegExp(word)}\\b`);
        return regex.test(alert.message);
    });
}

function matchesAddress(alert, addresses, addrIdx) {
    const addr = addresses[addrIdx];
    if (!addr) return false;
    
    // For Tauron, use isLocal flag
    if (alert.source === 'tauron') {
        return alert.isLocal === true;
    }
    
    // For Water and Fortum, use street name matching
    if (!alert.message) return false;
    return matchesStreetName(alert, addr.streetName);
}

function renderAlerts(alerts, container, settings, selectedAddrIdx = -1) {
    const now = new Date();

    const enabledSources = (settings && settings.enabledSources) ? settings.enabledSources : ['tauron', 'water', 'fortum'];
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
                if (addr && matchesStreetName(item, addr.streetName)) {
                    localWater.push(item);
                } else {
                    otherWater.push(item);
                }
            } else if (item.source === 'fortum') {
                const addr = addresses[selectedAddrIdx];
                if (addr && matchesStreetName(item, addr.streetName)) {
                    localFortum.push(item);
                } else {
                    otherFortum.push(item);
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
            }
        });
    } else {
        otherTauron = activeAlerts.filter(a => a.source === 'tauron');
        otherWater = activeAlerts.filter(a => a.source === 'water');
        otherFortum = activeAlerts.filter(a => a.source === 'fortum');
    }

    const hasLocalAlerts = localTauron.length > 0 || localWater.length > 0 || localFortum.length > 0;
    const hasOtherAlerts = otherTauron.length > 0 || otherWater.length > 0 || otherFortum.length > 0;
    const hasAnyAlerts = hasLocalAlerts || hasOtherAlerts;

    container.innerHTML = '';

    if (!hasAnyAlerts) {
        const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
        const msgNoLoc = typeof t !== 'undefined' ? t('msg_no_outages_local') : 'No planned outages for your location.';
        container.innerHTML = `<div class="section-label">${lblYourLoc}</div><div class="no-outages">${msgNoLoc}</div>`;
        return;
    }

    if (hasLocalAlerts) {
        const totalLocal = localTauron.length + localWater.length + localFortum.length;
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
            const lblSection = typeof t !== 'undefined' ? t('lbl_section_fortum') : 'Power (Fortum)';
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
    }
}

function renderCards(alerts, source) {
    const sourceLabel = source === 'water'
        ? (typeof t !== 'undefined' ? t('source_water') : '💧 Water Outage')
        : source === 'fortum'
        ? (typeof t !== 'undefined' ? t('source_fortum') : '⚡ Fortum Outage')
        : (typeof t !== 'undefined' ? t('source_tauron') : '⚡ Power Outage');

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

