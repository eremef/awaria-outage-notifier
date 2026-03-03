if (typeof document !== 'undefined') {
    document.addEventListener('DOMContentLoaded', () => {
        initSettings();
        initPullToRefresh();
        loadSettingsAndFetch();
    });
}

// ── Settings ──────────────────────────────────────────────

let currentSettings = null;

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
                language: 'system'
            };
        } else {
            currentSettings.theme = newTheme;
        }

        // Auto-save
        // We only save if we have a valid structure.
        // Even if location is empty, we save the preference.
        try {
            await window.__TAURI__.core.invoke('save_settings', {
                settings: currentSettings
            });
            console.log('Theme saved:', newTheme);
        } catch (error) {
            console.error('Failed to auto-save theme:', error);
        }
    });

    langSelect.addEventListener('change', async (e) => {
        const newLang = e.target.value;
        initLanguage(newLang); // comes from i18n.js
        applyTranslations();   // translates immediately

        if (!currentSettings) {
            currentSettings = {
                cityName: '',
                streetName: '',
                houseNo: '',
                cityGAID: 0,
                streetGAID: 0,
                theme: 'system',
                language: newLang
            };
        } else {
            currentSettings.language = newLang;
        }

        try {
            await window.__TAURI__.core.invoke('save_settings', {
                settings: currentSettings
            });
            // Re-render outages so dates format correctly
            if (document.getElementById('outages-container').innerHTML !== '' &&
                !document.getElementById('outages-container').querySelector('.no-outages, .error, .loading')) {
                // Ideally we shouldn't re-fetch unless needed, but easiest is to re-render what we have,
                // or just trigger fetchOutages again:
                fetchOutages();
            }
        } catch (error) {
            console.error('Failed to auto-save lang:', error);
        }
    });
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
            language
        };

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

        // Collapse settings and refresh outages
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
    if (theme === 'dark') {
        root.setAttribute('data-theme', 'dark');
    } else if (theme === 'light') {
        root.setAttribute('data-theme', 'light');
    } else {
        // System default
        if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
            root.setAttribute('data-theme', 'dark');
        } else {
            root.setAttribute('data-theme', 'light');
        }
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

// ── Outages ───────────────────────────────────────────────

async function fetchOutages() {
    const container = document.getElementById('outages-container');
    const lastUpdated = document.getElementById('last-updated');

    try {
        const data = await window.__TAURI__.core.invoke('fetch_outages');
        if (data.debug_query) {
            console.log('Fetch Outages Query:', data.debug_query);
        }
        lastUpdated.textContent = `${typeof t !== 'undefined' ? t('last_updated') : 'Last updated'}: ${new Date().toLocaleTimeString(typeof getLocaleString !== 'undefined' ? getLocaleString() : 'pl-PL')}`;
        renderOutages(data, container, currentSettings);
    } catch (error) {
        console.error('Error fetching data:', error);
        container.innerHTML = `<div class="error">${typeof t !== 'undefined' ? t('err_load_failed') : 'Failed to load outage data. Error: '}${error}</div>`;
    }
}

function filterOutages(allOutages, streetName, settings) {
    if (!allOutages) return [];

    // Normalize street name: remove "ul.", "al.", etc. and split into words
    const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
    const fullStreet = normalize(streetName);

    if (!fullStreet) return [];

    // Significant words are those with length >= 3
    const escapeRegExp = (string) => string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const significantWords = fullStreet.split(/\s+/).filter(word => word.length >= 3);

    return allOutages.filter(item => {
        // 1. Check GAID match if available
        if (settings && settings.streetGAID && item.GAID === settings.streetGAID) {
            return true;
        }

        if (!item.Message || !streetName) return false;

        const message = item.Message;

        // 2. Check full street name (original behavior)
        if (message.includes(streetName)) return true;

        // 3. Check significant words with word boundaries
        // This prevents "Main" from matching "Maintenance"
        return significantWords.some(word => {
            const regex = new RegExp(`\\b${escapeRegExp(word)}\\b`);
            return regex.test(message);
        });
    });
}

function renderOutages(data, container, settings) {
    const rawOutages = data.OutageItems || [];
    const now = new Date();

    // Global Filter: remove finished outages
    const allOutages = rawOutages.filter(item => {
        if (!item.EndDate) return true;
        const end = new Date(item.EndDate);
        return isNaN(end.getTime()) || end > now;
    });

    let streetName = '';
    if (settings && settings.streetName) {
        streetName = settings.streetName;
    }

    const localOutages = filterOutages(allOutages, streetName, settings);
    const localSet = new Set(localOutages);

    container.innerHTML = '';

    // Local outages section
    if (localOutages.length > 0) {
        const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
        container.innerHTML += `<div class="section-label">${lblYourLoc} (${localOutages.length})</div>`;
        container.innerHTML += renderCards(localOutages);
    } else {
        const msgNoLoc = typeof t !== 'undefined' ? t('msg_no_outages_local') : 'No planned outages for your location.';
        container.innerHTML += `<div class="no-outages">${msgNoLoc}</div>`;
    }

    // All outages section
    const otherOutages = allOutages.filter(item => !localSet.has(item));
    if (otherOutages.length > 0) {
        const lblOther = typeof t !== 'undefined' ? t('lbl_other_outages') : 'Other outages';
        container.innerHTML += `<div class="section-label other">${lblOther} (${otherOutages.length})</div>`;
        container.innerHTML += renderCards(otherOutages);
    }
}

function renderCards(outages) {
    const plannedLbl = typeof t !== 'undefined' ? t('lbl_planned_outage') : 'Planned Outage';
    return outages.map(item => `
        <div class="card">
            <span class="outage-type">${plannedLbl}</span>
            <div class="outage-time">
                ${formatDate(item.StartDate)} – ${formatDate(item.EndDate)}
            </div>
            ${item.Description ? `<div class="outage-reason">${item.Description}</div>` : ''}
            ${item.Message ? `<div class="outage-message">${item.Message}</div>` : ''}
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
        formatDate
    };
}
