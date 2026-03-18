if (typeof document !== 'undefined') {
    document.addEventListener('DOMContentLoaded', () => {
        initSettings();
        initPullToRefresh();
        initRefreshButton();
        loadSettingsAndFetch();
    });
}

// ── Settings ──────────────────────────────────────────────

let currentSettings = null;
let lastAlerts = [];
let lastFetchDate = null;

function initSettings() {
    const btn = document.getElementById('settings-btn');
    const panel = document.getElementById('settings-panel');
    const saveBtn = document.getElementById('save-settings-btn');
    const themeSelect = document.getElementById('theme-select');
    const langSelect = document.getElementById('language-select');

    btn.addEventListener('click', () => {
        panel.classList.toggle('hidden');
    });

    saveBtn.addEventListener('click', saveSettings);

    themeSelect.addEventListener('change', async (e) => {
        const newTheme = e.target.value;
        applyTheme(newTheme);

        // Update local state
        if (!currentSettings) {
            currentSettings = {
                cityName: '',
                streetName: '',
                houseNo: '',
                cityGAID: 0,
                streetGAID: 0,
                theme: newTheme,
                language: 'system',
                enabledSources: ['tauron', 'water', 'fortum']
            };
        } else {
            currentSettings.theme = newTheme;
        }

        await autoSaveSettings();
        const container = document.getElementById('outages-container');
        renderAlerts(lastAlerts || [], container, currentSettings);
        updateLastUpdated();
    });

    langSelect.addEventListener('change', async (e) => {
        const newLang = e.target.value;
        initLanguage(newLang);
        applyTranslations();

        if (!currentSettings) {
            currentSettings = {
                cityName: '',
                streetName: '',
                houseNo: '',
                cityGAID: 0,
                streetGAID: 0,
                theme: 'system',
                language: newLang,
                enabledSources: ['tauron', 'water', 'fortum']
            };
        } else {
            currentSettings.language = newLang;
        }

        await autoSaveSettings();
        
        // Re-render instantly from cache
        const container = document.getElementById('outages-container');
        renderAlerts(lastAlerts || [], container, currentSettings);
        updateLastUpdated();
    });

    // Location Toggle
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
                renderAlerts(lastAlerts || [], container, currentSettings);
                updateLastUpdated();
            });
        });
    });

}

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
            currentSettings = settings; // Store globally

            // Set language first so translations are correct
            if (settings.language && document.getElementById('language-select')) {
                document.getElementById('language-select').value = settings.language;
                initLanguage(settings.language);
            } else {
                initLanguage('system');
            }
            applyTranslations();

            document.getElementById('city-input').value = settings.cityName;
            document.getElementById('street-input').value = settings.streetName;
            document.getElementById('house-input').value = settings.houseNo;
            if (settings.theme) {
                document.getElementById('theme-select').value = settings.theme;
            }
            applyTheme(settings.theme || 'system');
            
            // Set alert sources
            const sources = settings.enabledSources || ['tauron', 'water', 'fortum'];
            document.getElementById('source-tauron-check').checked = sources.includes('tauron');
            document.getElementById('source-water-check').checked = sources.includes('water');
            document.getElementById('source-fortum-check').checked = sources.includes('fortum');

            // Collapse location if it looks valid
            if (settings.cityName && settings.cityGAID && settings.streetGAID) {
                document.getElementById('location-settings-collapsible').classList.add('collapsed');
            }

            fetchOutages();
        } else {
            initLanguage('system');
            applyTranslations();
            // No settings yet — show setup prompt
            const container = document.getElementById('outages-container');
            container.innerHTML = `<div class="no-outages">${typeof t !== 'undefined' ? t('setup_prompt') : 'Tap ⚙️ to configure your location.'}</div>`;
            document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';
            document.getElementById('settings-panel').classList.remove('hidden');

            // Apply default system theme but don't save yet
            applyTheme('system');
        }
    } catch (error) {
        console.error('Error loading settings:', error);
    }
}

async function saveSettings() {
    const cityName = document.getElementById('city-input').value.trim();
    const streetName = document.getElementById('street-input').value.trim();
    const houseNo = document.getElementById('house-input').value.trim();
    const theme = document.getElementById('theme-select').value;
    const language = document.getElementById('language-select').value;
    const status = document.getElementById('settings-status');

    if (!cityName || !streetName || !houseNo) {
        status.textContent = typeof t !== 'undefined' ? t('err_fields_required') : '⚠️ All fields are required.';
        status.className = 'settings-status error';
        return;
    }

    const saveBtn = document.getElementById('save-settings-btn');
    saveBtn.disabled = true;

    try {
        // Step 1: Lookup city
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

        // Step 2: Lookup street
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

        // Step 3: Save settings
        status.textContent = typeof t !== 'undefined' ? t('msg_saving') : '💾 Saving...';

        const newSettings = {
            cityName,
            streetName,
            houseNo,
            cityGAID: city.GAID,
            streetGAID: street.GAID,
            theme,
            language,
            enabledSources: []
        };
        if (document.getElementById('source-tauron-check').checked) newSettings.enabledSources.push('tauron');
        if (document.getElementById('source-water-check').checked) newSettings.enabledSources.push('water');
        if (document.getElementById('source-fortum-check').checked) newSettings.enabledSources.push('fortum');

        await window.__TAURI__.core.invoke('save_settings', {
            settings: newSettings
        });

        // Update global state
        currentSettings = newSettings;

        applyTheme(theme);
        initLanguage(language);
        applyTranslations();

        status.textContent = `${typeof t !== 'undefined' ? t('msg_saved') : '✅ Saved!'} ${city.GAID}, ${typeof t !== 'undefined' ? t('settings_street') + '=' : 'Street='}${street.GAID}`;
        status.className = 'settings-status success';

        // Collapse location section
        document.getElementById('location-settings-collapsible').classList.add('collapsed');

        // Collapse entire settings panel and refresh outages
        setTimeout(() => {
            document.getElementById('settings-panel').classList.add('hidden');
            status.textContent = '';
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
        lastAlerts = alerts; // Cache for instant re-rendering
        updateLastUpdated(new Date());
        renderAlerts(alerts, container, currentSettings);
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

function renderAlerts(alerts, container, settings) {
    const now = new Date();

    // Filter by enabled sources and finished status
    const enabledSources = (settings && settings.enabledSources) ? settings.enabledSources : ['tauron', 'water', 'fortum'];
    const activeAlerts = alerts.filter(item => {
        // Source filter
        if (!enabledSources.includes(item.source)) return false;

        // Date filter
        if (!item.endDate) return true;
        const end = new Date(item.endDate);
        return isNaN(end.getTime()) || end > now;
    });

    // Group by source
    const tauronAlerts = activeAlerts.filter(a => a.source === 'tauron');
    const waterAlerts = activeAlerts.filter(a => a.source === 'water');
    const fortumAlerts = activeAlerts.filter(a => a.source === 'fortum');

    // For Tauron, split into local vs other
    const streetName = (settings && settings.streetName) ? settings.streetName : '';

    const localTauron = streetName
        ? tauronAlerts.filter(a => {
            // Check by content matching
            if (a.message && a.message.includes(streetName)) return true;
            const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
            const fullStreet = normalize(streetName);
            const significantWords = fullStreet.split(/\s+/).filter(w => w.length >= 3);
            const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
            return significantWords.some(word => {
                const regex = new RegExp(`\\b${escapeRegExp(word)}\\b`);
                return a.message && regex.test(a.message);
            });
        })
        : [];
    const localTauronSet = new Set(localTauron);
    const otherTauron = tauronAlerts.filter(a => !localTauronSet.has(a));

    // For water, search by street name in content
    const localWater = streetName ? filterAlerts(waterAlerts, streetName) : [];
    const localWaterSet = new Set(localWater);
    const otherWater = waterAlerts.filter(a => !localWaterSet.has(a));

    // Fortum alerts (Wrocław only - currently no local filtering)
    const localFortum = streetName ? filterAlerts(fortumAlerts, streetName) : [];
    const localFortumSet = new Set(localFortum);
    const otherFortum = fortumAlerts.filter(a => !localFortumSet.has(a));

    container.innerHTML = '';

    const hasLocalAlerts = localTauron.length > 0 || localWater.length > 0 || localFortum.length > 0;

    // ── Your Location section ──
    if (hasLocalAlerts) {
        const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
        container.innerHTML += `<div class="section-label">${lblYourLoc} (${localTauron.length + localWater.length + localFortum.length})</div>`;
        container.innerHTML += renderCards(localTauron, 'tauron');
        container.innerHTML += renderCards(localWater, 'water');
        container.innerHTML += renderCards(localFortum, 'fortum');
    } else {
        const msgNoLoc = typeof t !== 'undefined' ? t('msg_no_outages_local') : 'No planned outages for your location.';
        container.innerHTML += `<div class="no-outages">${msgNoLoc}</div>`;
    }

    // ── Other Alerts Divider ──
    if (otherTauron.length > 0 || otherWater.length > 0 || otherFortum.length > 0) {
        const lblDivider = typeof t !== 'undefined' ? t('lbl_other_alerts_divider') : 'Other alerts';
        container.innerHTML += `<div class="other-divider"><span>${lblDivider}</span></div>`;
    }

    // ── Other Tauron section ──
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

    // ── Other Water section ──
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

    // ── Other Fortum section ──
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


    // If nothing at all
    if (activeAlerts.length === 0) {
        const msgNone = typeof t !== 'undefined' ? t('msg_no_alerts') : 'No active alerts.';
        container.innerHTML = `<div class="no-outages">${msgNone}</div>`;
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

