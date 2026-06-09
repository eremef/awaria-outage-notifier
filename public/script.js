const ICONS = {
    STAR: '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="icon-star"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>',
    STAR_OUTLINE: '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="icon-star"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>',
    EDIT: '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="icon-edit"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>',
    DELETE: '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="icon-trash"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>'
};

// Mock Tauri IPC for Home Assistant Ingress environment
if (typeof window !== 'undefined' && !window.__TAURI__) {
    console.warn('Tauri API not found. Assuming Home Assistant Ingress environment.');
    window.__TAURI__ = {
        core: {
            invoke: async (command, args) => {
                console.log(`[HA Mock] invoke: ${command}`, args);
                if (command === 'load_settings') {
                    const res = await fetch('api/settings');
                    if (!res.ok) throw new Error('Failed to fetch settings');
                    return await res.json();
                }
                if (command === 'fetch_all_alerts') {
                    const res = await fetch('api/alerts');
                    if (!res.ok) throw new Error('Failed to fetch alerts');
                    return await res.json(); // returns {alerts: [], is_stale, is_offline}
                }
                if (command === 'save_settings' || command === 'add_address' || command === 'remove_address' || command === 'set_primary_address') {
                    console.warn('[HA Mock] Settings are read-only in Home Assistant Add-on mode. Edit via Add-on Configuration tab.');
                    alert(typeof t !== 'undefined' ? t('ha_readonly_settings', 'Settings are read-only. Please edit them in the Home Assistant Add-on Configuration tab.') : 'Settings are read-only in HA Add-on mode.');
                    return typeof currentSettings !== 'undefined' ? currentSettings : {};
                }
                if (command === 'get_app_version') {
                    return "Home Assistant";
                }
                if (command.startsWith('teryt_')) {
                    return [];
                }
                return null;
            }
        },
        event: {
            listen: async () => {},
            emit: async () => {}
        }
    };

    // Hide settings button in HA mode
    document.addEventListener('DOMContentLoaded', () => {
        const settingsBtn = document.getElementById('settings-btn');
        if (settingsBtn) {
            settingsBtn.style.display = 'none';
        }
    });
}

if (typeof document !== 'undefined') {
    document.addEventListener('DOMContentLoaded', () => {
        // Initial theme pick from localStorage or system (prevents flash)
        applyTheme(localStorage.getItem('app-theme') || 'system');
        applyFontSize(localStorage.getItem('app-font-size') || 'small');

        initSettings();
        initPullToRefresh();
        initRefreshButton();
        initAddressFilter();
        initProgressConsole();
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

    // ── Settings ──────────────────────────────────────────────

    let currentSettings = null;
    let savedScrollY = 0;
    let lastAlerts = [];
    let lastFetchDate = null;
    let selectedAddressIndex = -1; // -1 means "all addresses"
    let isFetching = false;
    let fetchingSources = new Set();
    let consoleRefreshState = {
        isExpanded: false,
        total: 0,
        completed: 0,
        providers: {}
    };
    let isSearchingCities = false;
    let isSearchingStreets = false;
    let dateCache = {};
    let sourceLabelCache = {};

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
    const SOURCES = [
        // Power
        { id: 'enea', label: 'Enea', category: 'power', defaultNotify: true, i18nLabel: 'source_enea_name', i18nShort: 'source_enea_short' },
        { id: 'energa', label: 'Energa', category: 'power', defaultNotify: true, i18nLabel: 'source_energa_name', i18nShort: 'source_energa_short' },
        { id: 'pge', label: 'PGE', category: 'power', defaultNotify: true, i18nLabel: 'source_pge_name', i18nShort: 'source_pge_short' },
        { id: 'stoen', label: 'Stoen', category: 'power', defaultNotify: true, i18nLabel: 'source_stoen_name', i18nShort: 'source_stoen_short' },
        { id: 'tauron', label: 'Tauron', category: 'power', defaultNotify: true, i18nLabel: 'source_tauron_name', i18nShort: 'source_tauron_short' },
        // Gas
        { id: 'psg', label: 'PSG', category: 'gas', defaultNotify: true, i18nLabel: 'source_psg_name', i18nShort: 'source_psg_short' },
        // Heating
        { id: 'fortum', label: 'Fortum', category: 'heating', defaultNotify: true, i18nLabel: 'source_fortum_name', i18nShort: 'source_fortum_short' },
        { id: 'gpec', label: 'GPEC Gdańsk', category: 'heating', defaultNotify: true, i18nLabel: 'source_gpec_name', i18nShort: 'source_gpec_short' },
        { id: 'tauron_heat', label: 'Tauron Ciepło', category: 'heating', defaultNotify: true, i18nLabel: 'source_tauron_heat_name', i18nShort: 'source_tauron_heat_short' },
        { id: 'veolia_lodz', label: 'Veolia Łódź', category: 'heating', defaultNotify: true, i18nLabel: 'source_veolia_lodz_name', i18nShort: 'source_veolia_lodz_short' },
        { id: 'veolia_poznan', label: 'Veolia Poznań', category: 'heating', defaultNotify: true, i18nLabel: 'source_veolia_poznan_name', i18nShort: 'source_veolia_poznan_short' },
        { id: 'veolia_warszawa', label: 'Veolia Warszawa', category: 'heating', defaultNotify: true, i18nLabel: 'source_veolia_warszawa_name', i18nShort: 'source_veolia_warszawa_short' },
        // Water
        { id: 'aquanet', label: 'Aquanet', category: 'water', defaultNotify: true, i18nLabel: 'source_aquanet_name', i18nShort: 'source_aquanet_short' },
        { id: 'gdanskie_wodociagi', label: 'Gdańskie Wodociągi', category: 'water', defaultNotify: true, i18nLabel: 'source_gdanskie_wodociagi_name', i18nShort: 'source_gdanskie_wodociagi_short' },
        { id: 'katowickie_wodociagi', label: 'Katowickie Wodociągi', category: 'water', defaultNotify: true, i18nLabel: 'source_katowickie_wodociagi_name', i18nShort: 'source_katowickie_wodociagi_short' },
        { id: 'mpwik_warszawa', label: 'MPWiK Warszawa', category: 'water', defaultNotify: true, i18nLabel: 'source_mpwik_warszawa_name', i18nShort: 'source_mpwik_warszawa_short' },
        { id: 'mpwik_wroclaw', label: 'MPWiK Wrocław', category: 'water', defaultNotify: true, i18nLabel: 'source_mpwik_wroclaw_name', i18nShort: 'source_mpwik_wroclaw_short' },
        { id: 'puk_rokietnica', label: 'PUK Rokietnica', category: 'water', defaultNotify: true, i18nLabel: 'source_puk_rokietnica_name', i18nShort: 'source_puk_rokietnica_short' },
        { id: 'pwik_czestochowa', label: 'PWiK Częstochowa', category: 'water', defaultNotify: true, i18nLabel: 'source_pwik_czestochowa_name', i18nShort: 'source_pwik_czestochowa_short' },
        { id: 'pwik_kalisz', label: 'PWiK Kalisz', category: 'water', defaultNotify: true, i18nLabel: 'source_pwik_kalisz_name', i18nShort: 'source_pwik_kalisz_short' },
        { id: 'wmk', label: 'WMK', category: 'water', defaultNotify: true, i18nLabel: 'source_wmk_name', i18nShort: 'source_wmk_short' },
        { id: 'wodociagi_plockie', label: 'Wodociągi Płockie', category: 'water', defaultNotify: true, i18nLabel: 'source_wodociagi_plockie_name', i18nShort: 'source_wodociagi_plockie_short' },
        { id: 'zwik_lodz', label: 'ZWIK Łódź', category: 'water', defaultNotify: true, i18nLabel: 'source_zwik_lodz_name', i18nShort: 'source_zwik_lodz_short' },
    ];

    function renderSourcesUI() {
        const container = document.getElementById('sources-container');
        if (!container) return;

        const categories = {
            power: { label: 'Power', i18n: 'source_power' },
            heating: { label: 'Heat', i18n: 'source_heating' },
            water: { label: 'Water', i18n: 'source_water_name' },
            gas: { label: 'Gas', i18n: 'source_gas' }
        };

        const chevronSvg = `<svg class="chevron-icon" xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>`;

        let html = '';
        for (const [catId, catInfo] of Object.entries(categories)) {
            const catSources = SOURCES.filter(s => s.category === catId);
            if (catSources.length === 0) continue;

            html += `
            <div class="settings-field-group" id="group-${catId}">
                <div class="settings-group-header">
                    <button class="settings-group-header-clickable" type="button" aria-expanded="false" aria-controls="sources-list-${catId}">
                        ${chevronSvg}
                        <span class="settings-group-label" data-i18n="${catInfo.i18n}">${catInfo.label}</span>
                    </button>
                    <div class="master-checkbox-container">
                        <input type="checkbox" id="category-${catId}-check" title="Toggle all under ${catInfo.label}" aria-label="Toggle all under ${catInfo.label}">
                    </div>
                </div>
                <div class="settings-group-sources" id="sources-list-${catId}">
                    <div class="settings-field-row header indent">
                        <div class="source-group header-label" data-i18n="source_name">Source</div>
                        <div class="notify-group header-label" data-i18n="notify">Notify</div>
                    </div>
                    ${catSources.map(s => `
                        <div class="settings-field-row indent">
                            <div class="source-group checkbox-pair">
                                <input type="checkbox" id="source-${s.id}-check">
                                <label for="source-${s.id}-check" ${s.i18nLabel ? `data-i18n="${s.i18nLabel}"` : ''}>${s.label}</label>
                            </div>
                            <div class="notify-group checkbox-pair mini">
                                <input type="checkbox" id="notify-${s.id}-check" aria-label="${t('notify')} — ${s.label}">
                                <svg class="notify-bell-icon" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"></path><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"></path></svg>
                            </div>
                        </div>
                    `).join('')}
                </div>
            </div>
        `;
        }
        container.innerHTML = html;

        setupCategoryHeaderClickListeners();
        setupCategoryMasterCheckboxListeners();
    }

    function setupCategoryHeaderClickListeners() {
        const headers = document.querySelectorAll('.settings-group-header-clickable');
        headers.forEach(header => {
            header.addEventListener('click', () => {
                const group = header.closest('.settings-field-group');
                if (group) {
                    group.classList.toggle('expanded');
                    const isExpanded = group.classList.contains('expanded');
                    header.setAttribute('aria-expanded', isExpanded ? 'true' : 'false');
                }
            });
        });
    }

    function setupCategoryMasterCheckboxListeners() {
        const categories = ['power', 'heating', 'water', 'gas'];
        categories.forEach(catId => {
            const master = document.getElementById(`category-${catId}-check`);
            if (!master) return;

            master.addEventListener('click', () => {
                const checked = master.checked;
                const catSources = SOURCES.filter(s => s.category === catId);

                master.indeterminate = false;

                catSources.forEach(s => {
                    const cb = document.getElementById(`source-${s.id}-check`);
                    if (cb && cb.checked !== checked) {
                        cb.checked = checked;
                        cb.dispatchEvent(new Event('change'));
                    }
                });
            });
        });
    }

    function updateCategoryMasterCheck(catId) {
        const catSources = SOURCES.filter(s => s.category === catId);
        const children = catSources.map(s => document.getElementById(`source-${s.id}-check`)).filter(Boolean);
        const master = document.getElementById(`category-${catId}-check`);
        if (!master || children.length === 0) return;

        const checkedCount = children.filter(c => c.checked).length;
        if (checkedCount === children.length) {
            master.checked = true;
            master.indeterminate = false;
        } else if (checkedCount === 0) {
            master.checked = false;
            master.indeterminate = false;
        } else {
            master.checked = false;
            master.indeterminate = true;
        }
    }

    function updateAllCategoryMasterCheckboxes() {
        const categories = ['power', 'heating', 'water', 'gas'];
        categories.forEach(updateCategoryMasterCheck);
    }

    let editingAddressIndex = null;

    async function checkAndRequestNotificationPermission() {
        if (!window.__TAURI__) return;

        // Guard against about:blank origin which causes Tauri capability errors on startup
        if (window.location.href === 'about:blank' || window.location.href === 'http://localhost/') {
            return;
        }

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
                const settings = currentSettings || {};
                const notifyPrefs = settings.notificationPreferences || {};
                const hasAnyNotify = Object.values(notifyPrefs).some(v => v === true) || !!settings.upcomingNotificationEnabled;

                if (granted || !hasAnyNotify) {
                    warning.classList.add('hidden');
                } else {
                    warning.classList.remove('hidden');
                }
            }
        } catch (error) {
            console.error('Failed to check notification permission:', error);
        }
    }

    async function checkAndRequestBatteryOptimization(requestIfMissing = false) {
        if (!window.__TAURI__) return;

        try {
            const ignored = await window.__TAURI__.core.invoke('is_battery_optimization_ignored');

            const warning = document.getElementById('battery-optimization-warning');
            if (warning) {
                const settings = currentSettings || {};
                const notifyPrefs = settings.notificationPreferences || {};
                const hasAnyNotify = Object.values(notifyPrefs).some(v => v === true) || !!settings.upcomingNotificationEnabled;

                if (ignored || !hasAnyNotify) {
                    warning.classList.add('hidden');
                } else {
                    warning.classList.remove('hidden');
                }
            }

            if (!ignored && requestIfMissing) {
                await window.__TAURI__.core.invoke('request_battery_optimization_ignore');
            }
        } catch (error) {
            console.error('Failed to check/request battery optimization:', error);
        }
    }

    function updateUpcomingStatus() {
        const upcomingNotifyCheck = document.getElementById('upcoming-notify-check');
        const upcomingHoursInput = document.getElementById('upcoming-hours-input');
        const adjustContainer = document.getElementById('upcoming-adjust-container');
        const rowContainer = document.getElementById('upcoming-row-container');

        const anyNotifyChecked = SOURCES.some(s => {
            const cb = document.getElementById(`notify-${s.id}-check`);
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

    function updateNotifyStatus(sourceId, notifyId) {
        const sourceCheck = document.getElementById(sourceId);
        const notifyCheck = document.getElementById(notifyId);
        if (sourceCheck && notifyCheck) {
            notifyCheck.disabled = !sourceCheck.checked;
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

    function toggleSettings(forceState = null) {
        const btn = document.getElementById('settings-btn');
        const settingsView = document.getElementById('settings-view');
        const mainView = document.getElementById('main-view');
        if (!btn || !settingsView || !mainView) return;

        let shouldOpen;
        if (forceState !== null) {
            shouldOpen = forceState;
        } else {
            shouldOpen = !settingsView.classList.contains('open');
        }

        if (shouldOpen) {
            // Prepare for opening transition
            savedScrollY = window.scrollY;

            // Use 'fixed' during transition to isolate from document scroll/height changes.
            // This prevents Android safe-area flickering and 'edge-to-edge' jumps.
            settingsView.style.position = 'fixed';
            settingsView.style.top = '0';
            settingsView.style.display = 'flex';

            // Use requestAnimationFrame to ensure layout is ready before we start the transition
            requestAnimationFrame(() => {
                settingsView.classList.add('open');
            });

            // After transition, switch surfaces
            setTimeout(() => {
                if (settingsView.classList.contains('open')) {
                    mainView.classList.add('hidden');
                    // Switch back to relative so the view can scroll naturally with the main window
                    settingsView.style.position = 'relative';
                    window.scrollTo(0, 0);
                }
            }, 400); // Match CSS transition time
        } else {
            // Switch surfaces before closing transition
            mainView.classList.remove('hidden');

            // Keep settingsView fixed at the top while we restore the background scroll.
            // This hides the background jump from the user.
            settingsView.style.position = 'fixed';
            settingsView.style.top = '0';

            // On some Android WebViews, we need a small delay to ensure layout height is recalculated
            requestAnimationFrame(() => {
                requestAnimationFrame(() => {
                    window.scrollTo({
                        top: savedScrollY,
                        behavior: 'auto'
                    });

                    // Now that the background is restored under the opaque foreground, slide it out
                    settingsView.classList.remove('open');
                });
            });

            // After closing transition, clean up layout
            setTimeout(() => {
                if (!settingsView.classList.contains('open')) {
                    settingsView.style.display = 'none';
                    settingsView.style.position = 'absolute'; // Reset for next time
                }
            }, 400);
        }

        const isOpen = settingsView.classList.contains('open');

        // Update tooltip/title
        if (isOpen) {
            btn.setAttribute('data-i18n-title', 'settings_close');
        } else {
            btn.setAttribute('data-i18n-title', 'settings');
        }

        if (typeof applyTranslations === 'function') {
            applyTranslations();
        }

        if (isOpen) {
            // No need to reset settingsView.scrollTop as main window handles it now
            checkAndRequestNotificationPermission(); // Update permission warning state
            checkAndRequestBatteryOptimization(); // Update battery warning state
        }
    }

    window.toggleSettings = toggleSettings;

    function openSettingsTo(targetId) {
        toggleSettings(true);
        // Wait for the transition (400ms) + surface swap (relative position)
        setTimeout(() => {
            const target = document.getElementById(targetId);
            if (target) {
                target.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
        }, 600);
    }

    function initSettings() {
        renderSourcesUI();
        const btn = document.getElementById('settings-btn');
        const closeBtn = document.getElementById('settings-close-x');
        const saveBtn = document.getElementById('save-settings-btn');
        const themeSelect = document.getElementById('theme-select');
        const langSelect = document.getElementById('language-select');
        const fontSizeSelect = document.getElementById('font-size-select');
        const addAddressBtn = document.getElementById('add-address-btn');

        if (btn) btn.addEventListener('click', () => toggleSettings());
        if (closeBtn) closeBtn.addEventListener('click', () => toggleSettings(false));

        const bottomCloseBtn = document.getElementById('close-settings-btn');
        if (bottomCloseBtn) {
            bottomCloseBtn.addEventListener('click', () => toggleSettings(false));
        }

        const exportBtn = document.getElementById('export-settings-btn');
        if (exportBtn) {
            exportBtn.addEventListener('click', async () => {
                try {
                    const msg = await window.__TAURI__.core.invoke('export_settings');
                    if (msg) {
                        alert(msg);
                    }
                } catch (err) {
                    console.error('Export failed:', err);
                    if (err !== 'cancel' && err !== 'User cancelled') {
                        alert(t('err_export_failed') || 'Export failed');
                    }
                }
            });
        }

        const importBtn = document.getElementById('import-settings-btn');
        if (importBtn) {
            importBtn.addEventListener('click', async () => {
                try {
                    const imported = await window.__TAURI__.core.invoke('import_settings');
                    if (imported) {
                        alert(t('msg_import_success'));
                        window.location.reload();
                    }
                } catch (err) {
                    console.error('Import failed:', err);
                    if (err !== 'cancel' && err !== 'User cancelled') {
                        alert(t('err_import_failed'));
                    }
                }
            });
        }

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

        setupAutocompleteKeyboard(cityInput, document.getElementById('city-suggestions'));
        setupAutocompleteKeyboard(streetInput, document.getElementById('street-suggestions'));

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
                    enabledSources: [],
                    showOtherOutages: true
                };
            } else {
                currentSettings.theme = newTheme;
            }

            await autoSaveSettings();
            const container = document.getElementById('outages-container');
            renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
            updateLastUpdated();
        });

        if (fontSizeSelect) {
            fontSizeSelect.addEventListener('change', async (e) => {
                const newSize = e.target.value;
                applyFontSize(newSize);

                if (!currentSettings) {
                    currentSettings = {
                        addresses: [],
                        primaryAddressIndex: null,
                        theme: 'system',
                        language: 'system',
                        fontSize: newSize,
                        enabledSources: [],
                        showOtherOutages: true
                    };
                } else {
                    currentSettings.fontSize = newSize;
                }

                await autoSaveSettings();
                const container = document.getElementById('outages-container');
                renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
                updateLastUpdated();
            });
        }

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
                    enabledSources: [],
                    showOtherOutages: true
                };
            } else {
                currentSettings.language = newLang;
            }

            await autoSaveSettings();
            const container = document.getElementById('outages-container');
            renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
            updateLastUpdated();
        });



        SOURCES.forEach(s => {
            const sourceId = `source-${s.id}-check`;
            const notifyId = `notify-${s.id}-check`;
            const sourceCheckbox = document.getElementById(sourceId);
            if (sourceCheckbox) {
                sourceCheckbox.addEventListener('change', async () => {
                    if (!currentSettings) return;
                    const enabledSources = SOURCES
                        .filter(src => {
                            const cb = document.getElementById(`source-${src.id}-check`);
                            return cb && cb.checked;
                        })
                        .map(src => src.id);

                    currentSettings.enabledSources = enabledSources;
                    updateNotifyStatus(sourceId, notifyId);
                    updateUpcomingStatus();
                    updateCategoryMasterCheck(s.category);
                    await autoSaveSettings();
                    if (sourceCheckbox.checked) {
                        fetchOutages(s.id);
                        if (s.id === 'enea' || s.id === 'psg' || s.id === 'pge' || s.id === 'pwik_kalisz' || s.id === "gpec") {
                            showToast(typeof t !== 'undefined' ? t('toast_slow_fetching_warning') + ' ' + s.label : 'Fetching outages from this provider may take longer:');
                        }
                    } else {
                        const container = document.getElementById('outages-container');
                        renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
                    }
                });
            }

            const notifyCheckbox = document.getElementById(notifyId);
            if (notifyCheckbox) {
                notifyCheckbox.addEventListener('change', async () => {
                    if (!currentSettings) return;
                    if (!currentSettings.notificationPreferences) {
                        currentSettings.notificationPreferences = {};
                    }
                    currentSettings.notificationPreferences[s.id] = notifyCheckbox.checked;

                    if (notifyCheckbox.checked) {
                        await checkAndRequestNotificationPermission();
                        await checkAndRequestBatteryOptimization(true);
                    } else {
                        await checkAndRequestBatteryOptimization();
                    }

                    updateUpcomingStatus();
                    await autoSaveSettings();
                });
            }
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
                    if (upcomingNotifyCheck.checked) {
                        await checkAndRequestBatteryOptimization(true);
                    } else {
                        await checkAndRequestBatteryOptimization();
                    }
                    await autoSaveSettings();
                }
            });
        }

        if (upcomingHoursInput) {
            // Prevent manual typing of negative sign, scientific notation, or decimals
            upcomingHoursInput.addEventListener('keydown', (e) => {
                if (e.key === '-' || e.key === 'e' || e.key === '+' || e.key === '.') {
                    e.preventDefault();
                }
            });

            // Clamp values in real-time during typing
            upcomingHoursInput.addEventListener('input', () => {
                if (upcomingHoursInput.value !== '') {
                    const val = parseInt(upcomingHoursInput.value, 10);
                    if (val < 1) upcomingHoursInput.value = 1;
                    if (val > 168) upcomingHoursInput.value = 168;
                }
            });

            upcomingHoursInput.addEventListener('change', async () => {
                if (currentSettings) {
                    let val = parseInt(upcomingHoursInput.value, 10);
                    if (isNaN(val) || val < 1) val = 24;
                    if (val > 168) val = 168;
                    upcomingHoursInput.value = val;
                    currentSettings.upcomingNotificationHours = val;
                    await autoSaveSettings();
                }
            });
        }

        const showOtherCheck = document.getElementById('show-other-outages-check');
        if (showOtherCheck) {
            showOtherCheck.addEventListener('change', async () => {
                if (currentSettings) {
                    currentSettings.showOtherOutages = showOtherCheck.checked;
                    await autoSaveSettings();
                    const container = document.getElementById('outages-container');
                    renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
                }
            });
        }

        const filterByHouseCheck = document.getElementById('filter-by-house-no-check');
        if (filterByHouseCheck) {
            filterByHouseCheck.addEventListener('change', async () => {
                if (currentSettings) {
                    currentSettings.filterByHouseNo = filterByHouseCheck.checked;
                    if (filterByHouseCheck.checked) {
                        showToast(typeof t !== 'undefined' ? t('toast_house_no_filter_warning') : 'Warning: Filtering by house number might skip some outages if the provider\'s description is malformed.');
                    }
                    await autoSaveSettings();
                    const container = document.getElementById('outages-container');
                    renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
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

        const activeAddresses = (currentSettings && Array.isArray(currentSettings.addresses))
            ? currentSettings.addresses.map((addr, idx) => ({ ...addr, originalIndex: idx })).filter(addr => addr && addr.isActive !== false)
            : [];

        const activeCount = activeAddresses.length;
        console.log('updateAddressFilter: activeCount=', activeCount);

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
        console.log('renderAddressesList: currentSettings=', currentSettings);
        const list = document.getElementById('addresses-list');
        if (!list) return;

        if (!currentSettings || !Array.isArray(currentSettings.addresses) || currentSettings.addresses.length === 0) {
            list.innerHTML = `<div class="no-addresses">${typeof t !== 'undefined' ? t('no_addresses') : 'No addresses configured. Add one below.'}</div>`;
            return;
        }

        list.innerHTML = currentSettings.addresses.map((addr, idx) => `
        <div class="address-item ${addr.isActive === false ? 'disabled' : ''}">
            <div class="checkbox-pair mini" style="margin-right: 0.75rem; margin-top: 2px;">
                <input type="checkbox" ${addr.isActive !== false ? 'checked' : ''} onchange="toggleAddressActive(${idx})" title="${addr.isActive === false ? (typeof t !== 'undefined' ? t('lbl_address_disabled') : 'Disabled') : (typeof t !== 'undefined' ? t('lbl_address_active') : 'Active')}" aria-label="Toggle address status for ${escapeHtml(addr.streetName1 || addr.cityName || 'address')}">
            </div>
            <div class="address-info">
                <div class="address-name">${addr.name || (typeof t !== 'undefined' ? t('default_address_name') + ' ' + (idx + 1) : 'Address ' + (idx + 1))}</div>
                <div class="address-detail">${addr.streetName} ${addr.houseNo}, ${addr.cityName}</div>
            </div>
            <div class="address-actions">
                ${idx === currentSettings.primaryAddressIndex ? `<span class="primary-badge" title="Primary">${ICONS.STAR}</span>` : `<button class="icon-btn star-btn" onclick="setPrimaryAddress(${idx})" title="Set as primary">${ICONS.STAR_OUTLINE}</button>`}
                <button class="icon-btn edit-btn" onclick="editAddress(${idx})" title="Edit">${ICONS.EDIT}</button>
                <button class="icon-btn delete-btn" onclick="removeAddress(${idx})" title="Remove">${ICONS.DELETE}</button>
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
        const input = document.getElementById('city-input');
        if (!cities || cities.length === 0) {
            container.innerHTML = '<div class="suggestion-item no-results">No cities found</div>';
            container.classList.remove('hidden');
            if (input) input.setAttribute('aria-expanded', 'true');
            return;
        }

        container.innerHTML = cities.map((c, idx) => `
        <div class="suggestion-item" 
            id="city-opt-${idx}"
            role="option"
            data-city-id="${c.city_id}" 
            data-city-name="${escapeHtml(c.city)}"
            data-voivodeship="${escapeHtml(c.voivodeship)}"
            data-district="${escapeHtml(c.district)}"
            data-commune="${escapeHtml(c.commune)}">
            <div class="suggestion-name">${escapeHtml(c.city)}</div>
            <div class="suggestion-detail">${escapeHtml(c.voivodeship)} / ${escapeHtml(c.district)} / ${escapeHtml(c.commune)} / ${c.locality_type ? ` ${escapeHtml(c.locality_type)}` : ''}</div>
        </div>
    `).join('');

        if (input) input.setAttribute('aria-expanded', 'true');

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
        const input = document.getElementById('street-input');
        if (!streets || streets.length === 0) {
            container.innerHTML = '<div class="suggestion-item no-results">No streets found</div>';
            container.classList.remove('hidden');
            if (input) input.setAttribute('aria-expanded', 'true');
            return;
        }

        container.innerHTML = streets.map((s, idx) => `
        <div class="suggestion-item" 
            id="street-opt-${idx}"
            role="option"
            data-street-id="${s.street_id}" 
            data-street-name="${escapeHtml(s.full_street_name)}" 
            data-street-name1="${escapeHtml(s.street_name_1)}" 
            data-street-name2="${s.street_name_2 ? escapeHtml(s.street_name_2) : ''}">
            <div class="suggestion-name">${escapeHtml(s.full_street_name)}</div>
        </div>
    `).join('');

        if (input) input.setAttribute('aria-expanded', 'true');

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
        const inputId = id === 'city-suggestions' ? 'city-input' : 'street-input';
        const input = document.getElementById(inputId);
        if (input) {
            input.setAttribute('aria-expanded', 'false');
            input.removeAttribute('aria-activedescendant');
        }
    }

    function setupAutocompleteKeyboard(input, suggestionsContainer) {
        let activeIndex = -1;

        function getItems() {
            return suggestionsContainer.querySelectorAll('.suggestion-item:not(.no-results)');
        }

        function clearActive() {
            getItems().forEach(item => item.classList.remove('active'));
            activeIndex = -1;
            input.removeAttribute('aria-activedescendant');
        }

        input.addEventListener('keydown', (e) => {
            const items = getItems();
            if (suggestionsContainer.classList.contains('hidden') || items.length === 0) {
                return;
            }

            if (e.key === 'ArrowDown') {
                e.preventDefault();
                activeIndex = (activeIndex + 1) % items.length;
                highlightItem(items);
            } else if (e.key === 'ArrowUp') {
                e.preventDefault();
                activeIndex = (activeIndex - 1 + items.length) % items.length;
                highlightItem(items);
            } else if (e.key === 'Enter') {
                if (activeIndex >= 0 && activeIndex < items.length) {
                    e.preventDefault();
                    items[activeIndex].click();
                    clearActive();
                }
            } else if (e.key === 'Escape') {
                e.preventDefault();
                suggestionsContainer.classList.add('hidden');
                input.setAttribute('aria-expanded', 'false');
                clearActive();
            }
        });

        function highlightItem(items) {
            items.forEach((item, idx) => {
                if (idx === activeIndex) {
                    item.classList.add('active');
                    input.setAttribute('aria-activedescendant', item.id || `opt-${idx}`);
                    item.scrollIntoView({ block: 'nearest' });
                } else {
                    item.classList.remove('active');
                }
            });
        }

        const observer = new MutationObserver(() => {
            activeIndex = -1;
            input.removeAttribute('aria-activedescendant');
        });
        observer.observe(suggestionsContainer, { childList: true });
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
            if (!window.__TAURI__) {
                console.warn('Tauri API not found, skipping setting load');
                initLanguage('system');
                applyTranslations();
                return;
            }
            const settings = await window.__TAURI__.core.invoke('load_settings');
            console.log('loadSettingsAndFetch: received settings:', settings);
            if (settings) {
                currentSettings = settings;
                console.log('loadSettingsAndFetch: addresses count:', settings.addresses ? settings.addresses.length : 'undefined');

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

                if (settings.fontSize && document.getElementById('font-size-select')) {
                    document.getElementById('font-size-select').value = settings.fontSize;
                }
                applyFontSize(settings.fontSize || 'small');

                // If enabledSources is present, auto-add any new providers with defaultNotify: true
                // that were added after the user last saved settings (migration for existing users).
                const sources = settings.enabledSources || [];
                SOURCES.forEach(s => {
                    const cb = document.getElementById(`source-${s.id}-check`);
                    if (cb) cb.checked = sources.includes(s.id);
                });

                const notifyPrefs = settings.notificationPreferences || {};
                SOURCES.forEach(s => {
                    const cb = document.getElementById(`notify-${s.id}-check`);
                    if (cb) cb.checked = !!notifyPrefs[s.id];
                });

                // Update disabled status for all notify checkboxes
                SOURCES.forEach(s => {
                    const sourceId = `source-${s.id}-check`;
                    const notifyId = `notify-${s.id}-check`;
                    updateNotifyStatus(sourceId, notifyId);
                });

                if (typeof updateAllCategoryMasterCheckboxes === 'function') {
                    updateAllCategoryMasterCheckboxes();
                }

                if (document.getElementById('upcoming-notify-check')) {
                    document.getElementById('upcoming-notify-check').checked = !!settings.upcomingNotificationEnabled;
                }
                if (document.getElementById('upcoming-hours-input')) {
                    document.getElementById('upcoming-hours-input').value = settings.upcomingNotificationHours !== undefined ? settings.upcomingNotificationHours : 24;
                }

                if (typeof updateUpcomingStatus === 'function') {
                    updateUpcomingStatus();
                }

                if (document.getElementById('show-other-outages-check')) {
                    document.getElementById('show-other-outages-check').checked = settings.showOtherOutages !== false;
                }
                if (document.getElementById('filter-by-house-no-check')) {
                    document.getElementById('filter-by-house-no-check').checked = !!settings.filterByHouseNo;
                }

                // Check permissions/optimization warnings on load with a slight delay
                // to allow Tauri's internal WebView URL state to settle from about:blank
                setTimeout(() => {
                    checkAndRequestNotificationPermission();
                    checkAndRequestBatteryOptimization();
                }, 500);

                updateAddressFilter();
                renderAddressesList();
                document.getElementById('addresses-list').classList.remove('hidden');
                document.getElementById('add-address-btn').classList.remove('hidden');
                document.getElementById('address-form').classList.add('hidden');

                if (settings.addresses && settings.addresses.length > 0) {
                    // Fast load from SQLite persistent cache first, then fetch fresh in background
                    fetchOutages(null, true).then(() => {
                        fetchOutages(null, false);
                    });
                } else {
                    renderAlerts([], container, currentSettings, selectedAddressIndex);
                    document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';
                    toggleSettings(true);
                }
            } else {
                initLanguage('system');
                applyTranslations();
                currentSettings = {
                    addresses: [],
                    primaryAddressIndex: null,
                    theme: 'system',
                    language: 'system',
                    fontSize: 'small',
                    enabledSources: [],
                    showOtherOutages: true,
                    filterByHouseNo: false
                };

                // Explicitly uncheck and disable all source/notify pairs on first run
                SOURCES.forEach(s => {
                    const sc = document.getElementById(`source-${s.id}-check`);
                    const nc = document.getElementById(`notify-${s.id}-check`);
                    if (sc) sc.checked = false;
                    if (nc) {
                        nc.checked = false;
                        nc.disabled = true;
                        const notifyGroup = nc.closest('.notify-group');
                        if (notifyGroup) notifyGroup.classList.add('notify-disabled');
                    }
                });

                updateAddressFilter();
                renderAddressesList();
                renderAlerts([], container, currentSettings, selectedAddressIndex);
                document.getElementById('last-updated').textContent = typeof t !== 'undefined' ? t('not_configured') : 'Not configured';

                // Apply the default 'system' theme on first run
                applyTheme('system');

                toggleSettings(true);
            }
        } catch (error) {
            console.error('Error loading settings:', error);
        }
    }

    async function saveNewAddress() {
        const idx = (currentSettings?.addresses?.length || 0) + 1;
        const defaultName = (typeof t !== 'undefined' ? t('default_address_name') : 'Address') + ' ' + idx;
        const name = document.getElementById('address-name-input').value.trim() || defaultName;
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
                    toggleSettings(false);
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
        let effectiveTheme = theme;

        if (!theme || theme === 'system') {
            const isHighContrast = window.matchMedia && window.matchMedia('(prefers-contrast: more)').matches;
            if (isHighContrast) {
                effectiveTheme = 'high-contrast';
            } else {
                effectiveTheme = (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) ? 'dark' : 'light';
            }
            root.setAttribute('data-theme', effectiveTheme);
            localStorage.setItem('app-theme', 'system');
        } else {
            root.setAttribute('data-theme', theme);
            localStorage.setItem('app-theme', theme);
        }
    }

    function applyFontSize(size) {
        const root = document.documentElement;
        let effectiveSize = size;
        if (!size || size === 'system') {
            effectiveSize = 'small';
        }
        root.setAttribute('data-font-size', effectiveSize);
        localStorage.setItem('app-font-size', effectiveSize);
    }

    // Watch for system theme changes
    if (window.matchMedia) {
        const handleThemeChange = () => {
            const currentSetting = document.getElementById('theme-select');
            if (currentSetting && currentSetting.value === 'system') {
                applyTheme('system');
            }
        };
        window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', handleThemeChange);
        window.matchMedia('(prefers-contrast: more)').addEventListener('change', handleThemeChange);
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

    async function fetchOutages(specificSource = null, cachedOnly = false) {
        if (specificSource) {
            if (fetchingSources.has(specificSource)) return;
            fetchingSources.add(specificSource);
            updateRefreshProgressUI();
        } else {
            if (isFetching && !cachedOnly) return;
            if (!cachedOnly) {
                isFetching = true;

                // If it is a full fresh fetch, trigger progressive loading for all enabled sources
                const enabled = currentSettings && Array.isArray(currentSettings.enabledSources)
                    ? currentSettings.enabledSources
                    : [];

                if (enabled.length > 0) {
                    const progressEl = document.getElementById('refresh-progress');
                    if (progressEl) {
                        progressEl.classList.remove('hidden');
                    }

                    // Reset progressive console states
                    consoleRefreshState.completed = 0;
                    consoleRefreshState.total = enabled.length;
                    consoleRefreshState.providers = {};
                    enabled.forEach(sourceId => {
                        consoleRefreshState.providers[sourceId] = 'pending';
                    });

                    let completedCount = 0;
                    const totalCount = enabled.length;

                    updateRefreshProgressUI(completedCount, totalCount);

                    // Trigger all fetches in parallel
                    const fetchPromises = enabled.map(async (sourceId) => {
                        consoleRefreshState.providers[sourceId] = 'fetching';
                        updateRefreshProgressUI();

                        let success = false;
                        try {
                            await fetchOutages(sourceId, false);
                            success = true;
                        } catch (err) {
                            console.error(`Error in progressive fetch for ${sourceId}:`, err);
                        } finally {
                            completedCount++;
                            consoleRefreshState.providers[sourceId] = success ? 'success' : 'error';
                            updateRefreshProgressUI(completedCount, totalCount);
                        }
                    });

                    try {
                        await Promise.all(fetchPromises);
                    } catch (e) {
                        console.error('Error during progressive fetching:', e);
                    } finally {
                        setTimeout(() => {
                            isFetching = false;
                            if (progressEl) {
                                progressEl.classList.add('hidden');
                                // Reset title and progress bar
                                const titleEl = document.getElementById('progress-console-title');
                                if (titleEl) {
                                    titleEl.textContent = typeof t !== 'undefined' ? t('refreshing_providers') : 'Refreshing providers';
                                }
                                const barFillEl = document.getElementById('progress-console-bar-fill');
                                if (barFillEl) barFillEl.style.width = '0%';
                            }
                        }, 800); // 800ms success delay
                        updateLastUpdated(new Date(), false, false);
                    }
                    return;
                }
            }
        }

        const container = document.getElementById('outages-container');
        try {
            const invokeArgs = {
                sources: specificSource ? [specificSource] : null,
                cachedOnly: cachedOnly
            };
            const response = await window.__TAURI__.core.invoke('fetch_all_alerts', invokeArgs);
            let newAlerts = response.alerts;
            const isStale = response.is_stale;
            const isOffline = response.is_offline;

            if (Array.isArray(newAlerts)) {
                const seen = new Set();
                newAlerts = newAlerts.filter(a => {
                    if (!a.hash) return true;
                    if (seen.has(a.hash)) return false;
                    seen.add(a.hash);
                    return true;
                });
            }

            if (specificSource) {
                // Merge new alerts for this source into lastAlerts
                lastAlerts = (lastAlerts || []).filter(a => a.source !== specificSource).concat(newAlerts);
            } else {
                lastAlerts = newAlerts;
            }

            updateLastUpdated(new Date(), isStale, isOffline);
            renderAlerts(lastAlerts || [], container, currentSettings, selectedAddressIndex);
        } catch (error) {
            console.error('Error fetching data:', error);
            // Only show full error message on full fetch
            if (!specificSource && !cachedOnly) {
                const errorMsg = error === 'ERR_NO_INTERNET' ? (typeof t !== 'undefined' ? t('err_no_internet') : 'No internet connection.') : `${typeof t !== 'undefined' ? t('err_load_failed') : 'Failed to load alert data. Error: '}${error}`;
                container.innerHTML = `<div class="error">${errorMsg}</div>`;
            }
        } finally {
            if (specificSource) {
                fetchingSources.delete(specificSource);
                updateRefreshProgressUI();
            } else {
                if (!cachedOnly) {
                    isFetching = false;
                }
            }
        }
    }

    function updateRefreshProgressUI(completed = null, total = null) {
        const progressEl = document.getElementById('refresh-progress');
        if (!progressEl) return;

        // If completed counts are passed, track them
        if (completed !== null) consoleRefreshState.completed = completed;
        if (total !== null) consoleRefreshState.total = total;

        const currentCompleted = consoleRefreshState.completed;
        const currentTotal = consoleRefreshState.total;

        if (fetchingSources.size === 0 && completed === null && currentCompleted === currentTotal) {
            // Let the setTimeout in fetchOutages handle the fade-out; don't hide immediately
            return;
        }

        // Update the header text
        const titleEl = document.getElementById('progress-console-title');
        const prefix = typeof t !== 'undefined' ? t('refreshing_providers') : 'Refreshing providers';

        // Update the progress bar fill
        const barFillEl = document.getElementById('progress-console-bar-fill');

        if (!isFetching) {
            // Single source fetch fallback
            const activeNames = Array.from(fetchingSources).map(id => {
                const src = SOURCES.find(s => s.id === id);
                return src ? (t(src.i18nShort) || src.label) : id;
            }).join(', ');

            if (titleEl) {
                const singlePrefix = typeof t !== 'undefined' ? t('refresh_progress_prefix') : 'Refreshing';
                titleEl.textContent = activeNames ? `${singlePrefix} (${activeNames})...` : `${singlePrefix}...`;
            }

            if (barFillEl) barFillEl.style.width = '100%';

            // Hide details/chevron since it's a single source
            const toggleEl = document.getElementById('progress-console-toggle');
            if (toggleEl) toggleEl.style.pointerEvents = 'none';
            const chevronEl = document.getElementById('progress-chevron-icon');
            if (chevronEl) chevronEl.classList.add('hidden');
            const detailsEl = document.getElementById('progress-console-details');
            if (detailsEl) detailsEl.classList.add('hidden');
        } else {
            // Full progressive refresh
            const toggleEl = document.getElementById('progress-console-toggle');
            if (toggleEl) toggleEl.style.pointerEvents = 'auto';
            const chevronEl = document.getElementById('progress-chevron-icon');
            if (chevronEl) {
                chevronEl.classList.remove('hidden');
                if (consoleRefreshState.isExpanded) {
                    chevronEl.classList.add('expanded');
                } else {
                    chevronEl.classList.remove('expanded');
                }
            }

            if (titleEl) {
                titleEl.textContent = `${prefix} (${currentCompleted}/${currentTotal})...`;
            }

            if (barFillEl) {
                const percent = currentTotal > 0 ? (currentCompleted / currentTotal) * 100 : 0;
                barFillEl.style.width = `${percent}%`;
            }

            // Toggle expansion state in DOM
            const detailsEl = document.getElementById('progress-console-details');
            if (detailsEl) {
                if (consoleRefreshState.isExpanded) {
                    detailsEl.classList.remove('hidden');
                } else {
                    detailsEl.classList.add('hidden');
                }
            }
        }

        // Render the detailed wall of statuses inside grid
        renderProgressConsoleDetails();
    }

    function initProgressConsole() {
        const toggleEl = document.getElementById('progress-console-toggle');
        if (!toggleEl) return;

        toggleEl.addEventListener('click', () => {
            const detailsEl = document.getElementById('progress-console-details');
            const chevronEl = document.getElementById('progress-chevron-icon');
            if (!detailsEl || !chevronEl) return;

            consoleRefreshState.isExpanded = !consoleRefreshState.isExpanded;
            toggleEl.setAttribute('aria-expanded', consoleRefreshState.isExpanded);

            if (consoleRefreshState.isExpanded) {
                detailsEl.classList.remove('hidden');
                chevronEl.classList.add('expanded');
                renderProgressConsoleDetails();
            } else {
                detailsEl.classList.add('hidden');
                chevronEl.classList.remove('expanded');
            }
        });
    }

    function renderProgressConsoleDetails() {
        const listEl = document.getElementById('progress-providers-list');
        if (!listEl) return;

        const enabled = currentSettings && Array.isArray(currentSettings.enabledSources)
            ? currentSettings.enabledSources
            : [];

        // Remove elements for providers that are no longer enabled
        Array.from(listEl.children).forEach(child => {
            if (!enabled.includes(child.dataset.sourceId)) {
                child.remove();
            }
        });

        enabled.forEach(sourceId => {
            const src = SOURCES.find(s => s.id === sourceId);
            const label = src ? (t(src.i18nShort) || src.label) : sourceId;
            const status = consoleRefreshState.providers[sourceId] || 'pending';

            let badge = listEl.querySelector(`[data-source-id="${sourceId}"]`);
            if (!badge) {
                badge = document.createElement('div');
                badge.dataset.sourceId = sourceId;
                listEl.appendChild(badge);
            }

            // Only update DOM if className or HTML changes
            const newClassName = `provider-status-badge ${status}`;
            if (badge.className !== newClassName) {
                badge.className = newClassName;
            }

            let iconHtml = '';
            if (status === 'fetching') {
                iconHtml = '<span class="status-pulse-dot" style="display:inline-block;width:6px;height:6px;border-radius:50%;background:currentColor;"></span>';
            } else if (status === 'success') {
                iconHtml = '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0;"><polyline points="20 6 9 17 4 12"></polyline></svg>';
            } else if (status === 'error') {
                iconHtml = '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink:0;"><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>';
            } else {
                iconHtml = '<span class="status-idle-dot" style="display:inline-block;width:6px;height:6px;border-radius:50%;background:var(--secondary-text);opacity:0.3;"></span>';
            }

            const newHtml = `${iconHtml}<span>${label}</span>`;
            if (badge.innerHTML !== newHtml) {
                badge.innerHTML = newHtml;
            }
        });
    }

    function updateLastUpdated(date, isStale = false, isOffline = false) {
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
        let text = `${label}: ${lastFetchDate.toLocaleTimeString(localeStr)}`;

        if (isOffline) {
            const offlineLabel = typeof t !== 'undefined' ? t('msg_offline') : 'Offline mode';
            text = `${offlineLabel} ${lastFetchDate.toLocaleTimeString(localeStr)}`;
            el.style.color = 'var(--text-secondary)';
            el.style.fontStyle = 'italic';
        } else if (isStale) {
            const staleLabel = typeof t !== 'undefined' ? t('msg_using_cache') : 'Offline mode';
            text = `${staleLabel} ${lastFetchDate.toLocaleTimeString(localeStr)}`;
            el.style.color = 'var(--text-secondary)';
            el.style.fontStyle = 'italic';
        } else {
            el.style.color = '';
            el.style.fontStyle = '';
        }

        el.textContent = text;
    }

    const getPolishStreetStem = (street) => {
        if (!street) return '';
        const clean = street.trim();
        const len = clean.length;
        if (len <= 3) return clean;

        if (clean.toLowerCase().endsWith('ego') && len > 3) {
            return clean.slice(0, -3);
        }
        if ((clean.toLowerCase().endsWith('ej') || clean.toLowerCase().endsWith('ych') || clean.toLowerCase().endsWith('ich')) && len > 2) {
            return clean.slice(0, -2);
        }

        const last = clean.toLowerCase().slice(-1);
        if (['a', 'y', 'i', 'e', 'ą', 'ę'].includes(last)) {
            return clean.slice(0, -1);
        }

        return clean;
    };

    function filterAlerts(alerts, streetName) {
        if (!alerts || !streetName) return [];

        const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const wordMatch = (text, word) => {
            const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(word)}([^\\p{L}]|$)`, 'iu');
            if (regex.test(text)) return true;

            const stem = getPolishStreetStem(word);
            if (stem && stem !== word && stem.length >= 3) {
                const stemRegex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(stem)}[\\p{L}]{0,3}([^\\p{L}]|$)`, 'iu');
                if (stemRegex.test(text)) return true;
            }
            return false;
        };

        const normalize = (name) => name.replace(/^(ul\.|al\.|pl\.|os\.|rondo)\s*/i, '').trim();
        const fullStreet = normalize(streetName);
        const significantWords = fullStreet.split(/\s+/).filter(w => w.length >= 3);

        return alerts.filter(item => {
            if (!item.message) return false;
            return significantWords.some(word => wordMatch(item.message, word));
        });
    }

    function matchesAddress(alert, addresses, addrIdx) {
        const addr = addresses[addrIdx];
        if (!addr || addr.isActive === false) return false;

        // Trust backend evaluation if available
        if (alert.isLocal !== undefined && alert.isLocal !== null) {
            return alert.isLocal && (addrIdx === -1 || alert.addressIndex === addrIdx || alert.addressIndex === -1);
        }

        // Fallback for alerts that don't have isLocal from backend
        if (!alert.message) return false;
        return matchesStreetName(alert, addr);
    }

    function matchesStreetName(alert, addr) {
        if (!alert.message || !addr) return false;

        const message = alert.message;
        const streetName1 = addr.streetName1 || '';
        const streetName2 = addr.streetName2 || null;
        const cityName = addr.cityName || '';

        const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

        // Prevent cross-city street matches for multi-city providers
        if (cityName) {
            const singleCityProviders = ['mpwik_wroclaw', 'mpwik_warszawa', 'stoen', 'wmk', 'zwik_lodz', 'wodociagi_plockie', 'katowickie_wodociagi', 'pwik_kalisz', 'gdanskie_wodociagi', 'gpec', 'puk_rokietnica'];
            if (singleCityProviders.includes(alert.source)) {
                // For single-city providers, reject matching if the saved address is in a completely different city
                const cityLower = cityName.toLowerCase();
                if (alert.source === 'mpwik_wroclaw' && !cityLower.startsWith('wroc')) return false;
                if (alert.source === 'mpwik_warszawa' && !cityLower.startsWith('warsz')) return false;
                if (alert.source === 'wmk' && !cityLower.startsWith('krak')) return false;
                if (alert.source === 'zwik_lodz' && !cityLower.startsWith('łódź') && !cityLower.startsWith('lodz')) return false;
                if (alert.source === 'wodociagi_plockie' && !cityLower.startsWith('pło') && !cityLower.startsWith('plo')) return false;
                if (alert.source === 'pwik_kalisz' && !cityLower.startsWith('kalisz')) return false;
                if (alert.source === 'katowickie_wodociagi' && !cityLower.startsWith('katow')) return false;
                if (alert.source === 'puk_rokietnica' && !isRokietnica(addr)) return false;
                if ((alert.source === 'gdanskie_wodociagi' || alert.source === 'gpec') &&
                    !cityLower.startsWith('gdań') &&
                    !cityLower.startsWith('gdan') &&
                    !cityLower.startsWith('pruszcz') &&
                    !cityLower.startsWith('kolbudy') &&
                    !cityLower.startsWith('kowale')
                ) {
                    return false;
                }
            } else {
                const location = alert.location || '';
                const msg = alert.message || '';

                const normalizeStr = (s) => s.toLowerCase()
                    .replace(/ą/g, 'a').replace(/ć/g, 'c').replace(/ę/g, 'e')
                    .replace(/ł/g, 'l').replace(/ń/g, 'n').replace(/ó/g, 'o')
                    .replace(/ś/g, 's').replace(/ź/g, 'z').replace(/ż/g, 'z');

                const combined = normalizeStr(msg + ' ' + location);
                const cityNorm = normalizeStr(cityName);
                const cityBase = cityNorm.length > 3 ? cityNorm.substring(0, cityNorm.length - 1) : cityNorm;

                if (!combined.includes(cityBase)) {
                    return false;
                }
            }
        }

        // Check if the message indicates a locality-wide outage
        // Patterns like "m. Kraków", "cała miejscowość Kraków", "całe miasto Kraków"
        if (cityName) {
            const cityEscaped = escapeRegExp(cityName);
            const localityWideRegex = new RegExp(`(^|[^\\p{L}])(m\\.|cała miejscowość|całe miasto|cały obszar miejscowości)\\s*${cityEscaped}([^\\p{L}]|$)`, 'iu');
            if (localityWideRegex.test(message)) return true;
        }

        if (!streetName1) {
            // Fallback for cities without streets: match by city name in the message
            if (!cityName) return false;
            const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(cityName)}([^\\p{L}]|$)`, 'iu');
            return regex.test(message);
        }
        const wordMatch = (word) => {
            const regex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(word)}([^\\p{L}]|$)`, 'iu');
            if (regex.test(message)) return true;

            const stem = getPolishStreetStem(word);
            if (stem && stem !== word && stem.length >= 3) {
                const stemRegex = new RegExp(`(^|[^\\p{L}])${escapeRegExp(stem)}[\\p{L}]{0,3}([^\\p{L}]|$)`, 'iu');
                if (stemRegex.test(message)) return true;
            }
            return false;
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
    }


    let renderAlertsTimeout = null;
    let renderAlertsArgs = null;

    function renderAlerts(alerts, container, settings, selectedAddrIdx = -1) {
        renderAlertsArgs = { alerts, container, settings, selectedAddrIdx };
        if (renderAlertsTimeout) clearTimeout(renderAlertsTimeout);
        renderAlertsTimeout = setTimeout(() => {
            _renderAlerts(renderAlertsArgs.alerts, renderAlertsArgs.container, renderAlertsArgs.settings, renderAlertsArgs.selectedAddrIdx);
            renderAlertsTimeout = null;
        }, 100);
    }

    function _renderAlerts(alerts, container, settings, selectedAddrIdx = -1) {
        const expandedGroups = new Set();
        if (container) {
            container.querySelectorAll('.collapsible:not(.collapsed)').forEach(el => {
                const match = el.className.match(/(source-[a-zA-Z0-9_]+)/);
                if (match) {
                    const isOther = el.classList.contains('other-alert-group');
                    expandedGroups.add((isOther ? 'other-' : 'local-') + match[1]);
                }
            });
        }

        const now = new Date();
        const enabledSources = (settings && settings.enabledSources) ? settings.enabledSources : SOURCES.map(s => s.id);

        const seen = new Set();
        console.log('DEBUG ALERTS RECEIVED:', JSON.stringify(alerts)); const activeAlerts = alerts.filter(item => {
            if (!enabledSources.includes(item.source)) return false;
            if (item.hash) {
                if (seen.has(item.hash)) return false;
                seen.add(item.hash);
            }
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
                    toggleSettings(true);
                    setTimeout(() => {
                        const section = document.getElementById('location-settings-section');
                        const addBtn = document.getElementById('add-address-btn');
                        if (section && addBtn) {
                            addBtn.click();
                            section.scrollIntoView({ behavior: 'smooth', block: 'start' });
                        }
                    }, 600);
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
                    openSettingsTo('location-settings-section');
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
                    openSettingsTo('sources-settings-title');
                });
            }
            return;
        }

        const isWarszawa = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            return city.startsWith('warszawa') || city.startsWith('warsaw') || addr.cityId === 918123;
        };
        const isWroclaw = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            return city.startsWith('wrocław') || city.startsWith('wroclaw') || addr.cityId === 986283;
        };
        const isKrakow = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            return city.startsWith('kraków') || city.startsWith('krakow') || addr.cityId === 950463;
        };
        const isPoznan = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            const commune = (addr.commune || '').trim().toLowerCase();
            const poznanCommunes = [
                'poznań', 'poznan', 'czerwonak', 'dopiewo', 'kleszczewo', 'komorniki',
                'kórnik', 'kornik', 'luboń', 'lubon', 'mosina', 'murowana goślina',
                'murowana goslina', 'puszczykowo', 'rokietnica', 'suchy las', 'swarzędz',
                'swarzedz', 'tarnowo podgórne', 'tarnowo podgorne', 'brodnica'
            ];
            return poznanCommunes.some(c => city.startsWith(c) || commune.startsWith(c));
        };
        function isLodz(addr) {
            if (!addr || !addr.cityName) return false;
            let name = addr.cityName.trim().toLowerCase();
            return name.startsWith('łódź') || name.startsWith('lodz') || addr.cityId === 958153;
        }

        function isPlock(addr) {
            if (!addr || !addr.cityName) return false;
            let name = addr.cityName.trim().toLowerCase();
            return name.startsWith('płock') || name.startsWith('plock') || addr.cityId === 969400; // I'll just check startsWith
        }
        const isKalisz = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            return city.startsWith('kalisz') || addr.cityId === 936579;
        };
        const isCzestochowa = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            const czestochowaCommunes = [
                'częstochowa', 'czestochowa', 'blachownia', 'kłobuck', 'klobuck',
                'konopiska', 'miedźno', 'miedzno', 'mykanów', 'mykanow', 'olsztyn',
                'poczesna', 'rędziny', 'redziny'
            ];
            return czestochowaCommunes.some(c => city.startsWith(c));
        };
        const isGdansk = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            return city.startsWith('gdańsk') || city.startsWith('gdansk') || addr.cityId === 908123;
        };
        const isKatowice = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            return city.startsWith('katowice') || addr.cityId === 937474;
        };
        const isRokietnica = (addr) => {
            if (!addr) return false;
            const city = (addr.cityName || '').trim().toLowerCase();
            const commune = (addr.commune || '').trim().toLowerCase();
            const rokietnicaVillages = [
                'rokietnica', 'bytkowo', 'cerekwica', 'kiekrz', 'krzyszkowo', 'mrowino',
                'napachanie', 'przybroda', 'rostworowo', 'rogierówko', 'rogierowko',
                'sobota', 'starzyny', 'żydowo', 'zydowo', 'dalekie'
            ];
            return rokietnicaVillages.some(v => city.startsWith(v)) || commune.includes('rokietnica');
        };

        const hasAnyWarszawa = addresses.some(a => a.isActive !== false && isWarszawa(a));
        const hasAnyWroclaw = addresses.some(a => a.isActive !== false && isWroclaw(a));
        const hasAnyKrakow = addresses.some(a => a.isActive !== false && isKrakow(a));
        const hasAnyPoznan = addresses.some(a => a.isActive !== false && isPoznan(a));
        const hasAnyLodz = addresses.some(a => a.isActive !== false && isLodz(a));
        const hasAnyKalisz = addresses.some(a => a.isActive !== false && isKalisz(a));
        const hasAnyCzestochowa = addresses.some(a => a.isActive !== false && isCzestochowa(a));
        const hasAnyPlock = addresses.some(a => a.isActive !== false && isPlock(a));
        const hasAnyGdansk = addresses.some(a => a.isActive !== false && isGdansk(a));
        const hasAnyKatowice = addresses.some(a => a.isActive !== false && isKatowice(a));
        const hasAnyRokietnica = addresses.some(a => a.isActive !== false && isRokietnica(a));

        const localLists = {};
        const otherLists = {};
        SOURCES.forEach(s => {
            localLists[s.id] = [];
            otherLists[s.id] = [];
        });

        activeAlerts.forEach(item => {
            if (!localLists[item.source]) return;

            if (selectedAddrIdx >= 0) {
                const addr = addresses[selectedAddrIdx];
                if (!addr) return;
                if (matchesAddress(item, addresses, selectedAddrIdx)) {
                    localLists[item.source].push(item);
                } else {
                    if (item.source === 'mpwik_wroclaw') {
                        if (isWroclaw(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'wmk') {
                        if (isKrakow(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'aquanet' || item.source === 'veolia_poznan') {
                        if (isPoznan(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'veolia_lodz' || item.source === 'zwik_lodz') {
                        if (isLodz(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'wodociagi_plockie') {
                        if (isPlock(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'pwik_kalisz') {
                        if (isKalisz(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'pwik_czestochowa') {
                        if (isCzestochowa(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'gdanskie_wodociagi' || item.source === 'gpec') {
                        if (isGdansk(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'katowickie_wodociagi') {
                        if (isKatowice(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'puk_rokietnica') {
                        if (isRokietnica(addr)) otherLists[item.source].push(item);
                    } else if (item.source === 'stoen' || item.source === 'veolia' || item.source === 'mpwik_warszawa') {
                        if (isWarszawa(addr)) otherLists[item.source].push(item);
                    } else {
                        const itemAddr = (item.addressIndex !== undefined && item.addressIndex !== null) ? addresses[item.addressIndex] : null;
                        if (!itemAddr || itemAddr.cityName === addr.cityName) {
                            otherLists[item.source].push(item);
                        }
                    }
                }
            } else {
                const isLocal = addresses.some((addr, idx) => addr.isActive !== false && matchesAddress(item, addresses, idx));
                if (isLocal) {
                    localLists[item.source].push(item);
                } else {
                    if (item.source === 'mpwik_wroclaw') {
                        if (hasAnyWroclaw) otherLists[item.source].push(item);
                    } else if (item.source === 'wmk') {
                        if (hasAnyKrakow) otherLists[item.source].push(item);
                    } else if (item.source === 'aquanet' || item.source === 'veolia_poznan') {
                        if (hasAnyPoznan) otherLists[item.source].push(item);
                    } else if (item.source === 'veolia_lodz' || item.source === 'zwik_lodz') {
                        if (hasAnyLodz) otherLists[item.source].push(item);
                    } else if (item.source === 'wodociagi_plockie') {
                        if (hasAnyPlock) otherLists[item.source].push(item);
                    } else if (item.source === 'pwik_kalisz') {
                        if (hasAnyKalisz) otherLists[item.source].push(item);
                    } else if (item.source === 'pwik_czestochowa') {
                        if (hasAnyCzestochowa) otherLists[item.source].push(item);
                    } else if (item.source === 'gdanskie_wodociagi' || item.source === 'gpec') {
                        if (hasAnyGdansk) otherLists[item.source].push(item);
                    } else if (item.source === 'katowickie_wodociagi') {
                        if (hasAnyKatowice) otherLists[item.source].push(item);
                    } else if (item.source === 'puk_rokietnica') {
                        if (hasAnyRokietnica) otherLists[item.source].push(item);
                    } else if (item.source === 'stoen' || item.source === 'veolia' || item.source === 'mpwik_warszawa') {
                        if (hasAnyWarszawa) otherLists[item.source].push(item);
                    } else {
                        otherLists[item.source].push(item);
                    }
                }
            }
        });

        const hasLocalAlerts = Object.values(localLists).some(l => l.length > 0);
        const hasOtherAlerts = Object.values(otherLists).some(l => l.length > 0);
        const showOther = settings.showOtherOutages !== false;
        const hasAnyAlerts = hasLocalAlerts || (hasOtherAlerts && showOther);

        let html = '';
        if (!hasAnyAlerts) {
            // ... (existing all-clear rendering) ...
            const title = typeof t !== 'undefined' ? t('all_clear_title') : 'Everything looks good!';
            const subtitle = typeof t !== 'undefined' ? t('all_clear_subtitle') : 'No outages detected.';
            const providersLbl = typeof t !== 'undefined' ? t('monitored_providers') : 'Monitored Providers';
            const refreshLbl = typeof t !== 'undefined' ? t('refresh_now') : 'Refresh Now';

            const statusItems = enabledSources.map(srcId => {
                const s = SOURCES.find(s => s.id === srcId);
                const name = s ? (typeof t !== 'undefined' ? t(s.i18nShort) : s.label) : srcId;
                return `
                <div class="status-item">
                    <div class="status-dot"></div>
                    <div class="status-info">
                        <span class="status-name">${escapeHtml(name)}</span>
                    </div>
                </div>
            `;
            }).join('');

            container.innerHTML = `
            <div class="all-clear-view">
                <div class="all-clear-title">${escapeHtml(title)}</div>
                <div class="all-clear-subtitle">${escapeHtml(subtitle)}</div>
                
                <div class="section-label" style="width:100%; max-width:450px; margin-bottom:1rem; text-align:left;">
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

        // Step 1: Render Local Alerts immediately
        if (hasLocalAlerts) {
            const totalLocal = Object.values(localLists).reduce((sum, l) => sum + l.length, 0);
            const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
            const titleCollapse = typeof t !== 'undefined' ? t('btn_collapse_expand_all') : 'Collapse/Expand All';
            html += `
            <div class="section-your-location">
                <span>${escapeHtml(lblYourLoc)} (${totalLocal})</span>
                <button class="collapse-local-btn" onclick="toggleLocalCollapse(this)" title="${escapeHtml(titleCollapse)}">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="6 9 12 15 18 9"></polyline>
                    </svg>
                </button>
            </div>`;

            SOURCES.forEach(s => {
                const list = localLists[s.id];
                if (list && list.length > 0) {
                    const lblSection = (typeof t !== 'undefined' ? t(`lbl_section_${s.id}`) : null) || `${s.category} (${s.label})`;
                    const groupId = `local-source-${s.id}`;
                    const isExpanded = expandedGroups.has(groupId);
                    const collapsedClass = isExpanded ? '' : ' collapsed';
                    html += `
                    <div class="collapsible local-alert-group source-${s.id}${collapsedClass}">
                        <button class="section-label other" type="button" aria-expanded="${isExpanded ? 'true' : 'false'}" aria-controls="local-content-${s.id}" onclick="this.parentElement.classList.toggle('collapsed'); this.setAttribute('aria-expanded', !this.parentElement.classList.contains('collapsed'))">
                            <span>${escapeHtml(lblSection)} (${list.length})</span>
                            <span class="toggle-icon">▼</span>
                        </button>
                        <div class="collapsible-content" id="local-content-${s.id}">
                            ${renderCards(list, s.id)}
                        </div>
                    </div>
                `;
                }
            });
        } else if (hasOtherAlerts) {
            const lblYourLoc = typeof t !== 'undefined' ? t('lbl_your_location') : 'Your location';
            const msgNoLocal = typeof t !== 'undefined' ? t('msg_no_outages_local') : 'No local alerts found.';
            html += `
            <div class="section-your-location"><span>${escapeHtml(lblYourLoc)} (0)</span></div>
            <div class="no-outages">${escapeHtml(msgNoLocal)}</div>
        `;
        }

        // Step 2: Render "Other Alerts" synchronously to prevent blinking
        if (hasOtherAlerts && showOther) {
            const lblDivider = typeof t !== 'undefined' ? t('lbl_other_alerts_divider') : 'Other alerts';
            const titleCollapse = typeof t !== 'undefined' ? t('btn_collapse_expand_all') : 'Collapse/Expand All';
            html += `
            <div class="other-divider">
                <span>${escapeHtml(lblDivider)}</span>
                <button class="collapse-local-btn" onclick="toggleOtherCollapse(this)" title="${escapeHtml(titleCollapse)}">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="6 9 12 15 18 9"></polyline>
                    </svg>
                </button>
            </div>`;

            SOURCES.forEach(s => {
                const list = otherLists[s.id];
                if (list && list.length > 0) {
                    const lblSection = (typeof t !== 'undefined' ? t(`lbl_section_${s.id}`) : null) || `${s.category} (${s.label})`;
                    const groupId = `other-source-${s.id}`;
                    const isExpanded = expandedGroups.has(groupId);
                    const collapsedClass = isExpanded ? '' : ' collapsed';
                    html += `
                    <div class="collapsible other-alert-group source-${s.id}${collapsedClass}">
                        <button class="section-label other" type="button" aria-expanded="${isExpanded ? 'true' : 'false'}" aria-controls="other-content-${s.id}" onclick="this.parentElement.classList.toggle('collapsed'); this.setAttribute('aria-expanded', !this.parentElement.classList.contains('collapsed'))">
                            <span>${escapeHtml(lblSection)} (${list.length})</span>
                            <span class="toggle-icon">▼</span>
                        </button>
                        <div class="collapsible-content" id="other-content-${s.id}">
                            ${renderCards(list, s.id)}
                        </div>
                    </div>
                `;
                }
            });
        }

        container.innerHTML = html;
    }

    window.toggleOtherCollapse = function (btn) {
        const groups = document.querySelectorAll('.other-alert-group');
        if (!groups.length) return;

        const anyExpanded = Array.from(groups).some(g => !g.classList.contains('collapsed'));
        const svg = btn.querySelector('polyline');

        if (anyExpanded) {
            groups.forEach(g => {
                g.classList.add('collapsed');
                const b = g.querySelector('.section-label.other');
                if (b) b.setAttribute('aria-expanded', 'false');
            });
            if (svg) svg.setAttribute('points', '6 9 12 15 18 9');
        } else {
            groups.forEach(g => {
                g.classList.remove('collapsed');
                const b = g.querySelector('.section-label.other');
                if (b) b.setAttribute('aria-expanded', 'true');
            });
            if (svg) svg.setAttribute('points', '18 15 12 9 6 15');
        }
    };

    window.toggleLocalCollapse = function (btn) {
        const groups = document.querySelectorAll('.local-alert-group');
        if (!groups.length) return;

        const anyExpanded = Array.from(groups).some(g => !g.classList.contains('collapsed'));
        const svg = btn.querySelector('polyline');

        if (anyExpanded) {
            groups.forEach(g => {
                g.classList.add('collapsed');
                const b = g.querySelector('.section-label.other');
                if (b) b.setAttribute('aria-expanded', 'false');
            });
            if (svg) svg.setAttribute('points', '6 9 12 15 18 9');
        } else {
            groups.forEach(g => {
                g.classList.remove('collapsed');
                const b = g.querySelector('.section-label.other');
                if (b) b.setAttribute('aria-expanded', 'true');
            });
            if (svg) svg.setAttribute('points', '18 15 12 9 6 15');
        }
    };

    function renderCards(alerts, sourceId) {
        let sourceLabel = sourceLabelCache[sourceId];
        if (!sourceLabel) {
            const s = SOURCES.find(src => src.id === sourceId);
            sourceLabel = sourceId;
            if (s) {
                sourceLabel = (typeof t !== 'undefined' ? t(s.i18nLabel) : null) || s.label;
                if (s.category === 'water') sourceLabel = '💧 ' + sourceLabel;
                else if (s.category === 'heating') sourceLabel = '🌡️ ' + sourceLabel;
                else if (s.category === 'gas') sourceLabel = '🔥 ' + sourceLabel;
                else sourceLabel = '⚡ ' + sourceLabel;
            }
            sourceLabelCache[sourceId] = sourceLabel;
        }

        return alerts.map(item => `
        <div class="card source-${item.source}" ${item.hash ? `data-hash="${item.hash}"` : ''}>
            <span class="outage-type">${escapeHtml(sourceLabel)}</span>
            <div class="outage-time">
                ${formatDate(item.startDate)} – ${formatDate(item.endDate)}
            </div>
            ${item.location ? `<div class="outage-location">${escapeHtml(item.location)}</div>` : ''}
            ${item.message ? `<div class="outage-message">${escapeHtml(item.message)}</div>` : ''}
        </div>
    `).join('');
    }

    function formatDate(dateString) {
        if (!dateString) return '';
        const localeStr = typeof getLocaleString !== 'undefined' ? getLocaleString() : 'pl-PL';
        const cacheKey = `${dateString}_${localeStr}`;
        if (dateCache[cacheKey]) return dateCache[cacheKey];

        const date = new Date(dateString);
        if (isNaN(date.getTime())) {
            return dateString;
        }

        const formatted = date.toLocaleString(localeStr, {
            weekday: 'short',
            day: 'numeric',
            month: 'short',
            hour: '2-digit',
            minute: '2-digit'
        });
        dateCache[cacheKey] = formatted;
        return formatted;
    }

    // Export for tests
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = {
            filterAlerts,
            formatDate,
            matchesStreetName,
            renderAlerts: _renderAlerts,
            updateNotifyStatus,
            updateUpcomingStatus,
            matchesAddress,
            escapeHtml,
            setCurrentSettings: (s) => { currentSettings = s; },
            setLastAlerts: (a) => { lastAlerts = a; },
            setSelectedAddressIndex: (i) => { selectedAddressIndex = i; }
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

    function showToast(message) {
        // Remove existing toast if any
        const existing = document.getElementById('app-toast');
        if (existing) {
            existing.remove();
        }

        const toast = document.createElement('div');
        toast.id = 'app-toast';
        toast.className = 'app-toast';

        const toastContent = document.createElement('div');
        toastContent.className = 'app-toast-content';

        // Add icon
        const iconSpan = document.createElement('span');
        iconSpan.className = 'app-toast-icon';
        iconSpan.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></svg>';
        toastContent.appendChild(iconSpan);

        // Add text
        const textSpan = document.createElement('span');
        textSpan.className = 'app-toast-text';
        textSpan.textContent = message;
        toastContent.appendChild(textSpan);

        // Add close button
        const closeBtn = document.createElement('button');
        closeBtn.className = 'app-toast-close';
        closeBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>';
        closeBtn.onclick = () => {
            toast.classList.remove('show');
            toast.classList.add('hide');
            setTimeout(() => toast.remove(), 400);
        };
        toastContent.appendChild(closeBtn);

        toast.appendChild(toastContent);
        document.body.appendChild(toast);

        // Force reflow to trigger animation
        toast.offsetHeight;
        toast.classList.add('show');

        // Auto remove after 6 seconds
        setTimeout(() => {
            if (toast.parentNode) {
                toast.classList.remove('show');
                toast.classList.add('hide');
                setTimeout(() => {
                    if (toast.parentNode) toast.remove();
                }, 400);
            }
        }, 6000);
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
}
