// i18n.js
// Basic translation dictionary and helper functions for frontend

const translations = {
    en: {
        "title": "Awaria",
        "settings_appearance": "General settings",
        "notify": "Notifications",
        "source_name": "Provider",
        "settings_language": "Language",
        "settings": "Settings",
        "lang_system": "System Default",
        "lang_en": "English",
        "lang_pl": "Polski",
        "settings_theme": "Theme",
        "theme_system": "System Default",
        "theme_light": "Light",
        "theme_dark": "Dark",
        "theme_emerald": "Emerald (Light)",
        "theme_ocean": "Ocean (Dark)",
        "theme_nord": "Nord (Arctic Dark)",
        "theme_dracula": "Dracula (Vampire)",
        "theme_high_contrast": "High Contrast (AAA)",
        "theme_sepia": "Sepia (Warm Light)",
        "theme_latte": "Latte (Pastel Light)",
        "theme_monochrome_light": "Monochrome (Light)",
        "theme_monochrome_dark": "Monochrome (Dark)",
        "settings_font_size": "Font Size",
        "font_size_small": "Small",
        "font_size_medium": "Medium",
        "font_size_large": "Large",
        "settings_location": "Location Settings",
        "settings_city": "City",
        "settings_city_placeholder": "e.g. Wrocław",
        "settings_street": "Street",
        "settings_street_placeholder": "e.g. Kuźnicza",
        "settings_house": "House No",
        "settings_house_placeholder": "e.g. 25 (default 1)",
        "settings_save": "Save",
        "refresh_pull": "↻ Release to refresh",
        "loading_data": "Loading alert data...",
        "last_updated": "Last updated",
        "checking_updates": "Checking for updates...",
        "not_configured": "Not configured",
        "setup_prompt": "Tap ⚙️ to configure your location.",
        "err_fields_required": "⚠️ All fields are required.",
        "hint_select_from_list": "Please select from the list.",
        "msg_looking_city": "🔍 Looking up city...",
        "msg_looking_street": "🔍 Looking up street...",
        "msg_saving": "💾 Saving...",
        "msg_saved": "✅ Saved!",
        "err_city_not_found": "❌ City not found. Did you mean: ",
        "err_street_not_found": "❌ Street not found. Did you mean: ",
        "err_load_failed": "Failed to load alert data. Error: ",
        "lbl_your_location": "Your location",
        "lbl_other_outages": "Other outages",
        "msg_no_outages_local": "No local alerts found.",
        "lbl_planned_outage": "Planned Outage",

        "msg_no_alerts": "No active alerts.",
        "all_clear_title": "Everything looks good!",
        "all_clear_subtitle": "No outages detected in your monitored areas.",
        "monitored_providers": "Monitored Providers",
        "refresh_now": "Refresh Now",
        "settings_sources": "Monitored Utilities",
        "source_power": "Power",
        "source_heating": "Heat",
        "source_gas": "Gas",
        "source_tauron_name": "Tauron",
        "source_fortum_name": "Fortum",
        "source_energa_name": "Energa",
        "source_enea_name": "Enea",
        "source_pge_name": "PGE",
        "source_water_name": "Water",
        "source_stoen_name": "Stoen",
        "source_psg_name": "PSG",
        "source_wmk_name": "WMK Kraków",
        "source_aquanet_name": "Aquanet",
        "source_tauron_heat_name": "Tauron Ciepło",
        "source_veolia_warszawa_name": "Veolia Warszawa",
        "source_veolia_poznan_name": "Veolia Poznań",
        "source_veolia_lodz_name": "Veolia Łódź",
        "source_zwik_lodz_name": "ZWiK Łódź",
        "source_wodociagi_plockie_name": "Wodociągi Płockie",
        "source_katowickie_wodociagi_name": "Katowickie Wodociągi",
        "source_pwik_kalisz_name": "PWiK Kalisz",
        "source_pwik_czestochowa_name": "PWiK Częstochowa",
        "source_gdanskie_wodociagi_name": "Gdańskie Wodociągi",
        "source_gpec_name": "GPEC Gdańsk",
        "source_puk_rokietnica_name": "PUK Rokietnica",
        "source_tauron_short": "Tauron",
        "source_mpwik_wroclaw_name": "MPWiK Wrocław",
        "source_mpwik_wroclaw_short": "MPWiK WRO",
        "source_mpwik_warszawa_name": "MPWiK Warszawa",
        "source_mpwik_warszawa_short": "MPWiK WAW",
        "source_fortum_short": "Fortum",
        "source_energa_short": "Energa",
        "source_enea_short": "Enea",
        "source_stoen_short": "Stoen",
        "source_pge_short": "PGE",
        "source_psg_short": "PSG",
        "source_wmk_short": "WMK",
        "source_aquanet_short": "Aquanet",
        "source_tauron_heat_short": "Tauron Heat",
        "source_veolia_warszawa_short": "Veolia Warszawa",
        "source_veolia_poznan_short": "Veolia Poznań",
        "source_veolia_lodz_short": "Veolia Łódź",
        "source_zwik_lodz_short": "ZWiK Łódź",
        "source_wodociagi_plockie_short": "Wodociągi Płockie",
        "source_katowickie_wodociagi_short": "Kat. Wodociągi",
        "source_pwik_kalisz_short": "PWiK Kalisz",
        "source_pwik_czestochowa_short": "PWiK Częstochowa",
        "source_gdanskie_wodociagi_short": "Gdańsk Water",
        "source_gpec_short": "GPEC",
        "source_puk_rokietnica_short": "PUK Rokietnica",
        "lbl_other_alerts_divider": "Other locations",
        "addr_filter_all": "All addresses",
        "lbl_address_active": "Active",
        "lbl_address_disabled": "Disabled",
        "default_address_name": "Address",
        "address_name": "Name",
        "address_name_placeholder": "e.g. Home, Work",
        "no_addresses": "No addresses configured. Add one below.",
        "no_streets": "No streets",
        "edit_address": "Edit Address",
        "save_changes": "Save Changes",
        "cancel": "Cancel",
        "footer_copyright": "© %YEAR% <a href=\"https://eremef.xyz\" target=\"_blank\" rel=\"noopener noreferrer\" class=\"external-link\">eremef</a>",
        "footer_version": "Awaria V%VERSION%",
        "footer_github": "Source on GitHub",
        "cuplink_support": "Enjoying this app? Buy me a virtual coffee to support its development!",
        "empty_state_title": "Welcome to Awaria",
        "empty_state_subtitle": "Start by adding your first location to monitor for power, water, and heat outages.",
        "empty_state_cta": "Add Address",
        "disabled_state_title": "Monitoring Paused",
        "disabled_state_subtitle": "All your saved locations are currently disabled. Enable them in settings to see outages.",
        "disabled_state_cta": "Open Settings",
        "sources_disabled_state_title": "Alerts Disabled",
        "sources_disabled_state_subtitle": "No alert sources are enabled. Enable them in settings to see outages.",
        "settings_upcoming_title": "Upcoming Outages - additional notifications",
        "settings_upcoming_prefix": "Notify",
        "settings_upcoming_suffix": "h before outage start",
        "settings_notification_permission_warning": "System notifications are disabled. Please enable them in Android settings to receive alerts.",
        "settings_battery_optimization_warning": "Battery optimizations are enabled. Please set to \"Unrestricted\" in Android settings to ensure reliable background updates.",
        "settings_backup": "Backup & Restore",
        "settings_export": "Export Settings",
        "settings_import": "Import Settings",
        "msg_export_success": "Settings exported successfully!",
        "msg_import_success": "Settings imported successfully! Reloading...",
        "err_export_failed": "Failed to export settings.",
        "err_import_failed": "Failed to import settings. Invalid file.",
        "widget_config_title": "Select address for widget",
        "widget_config_primary": "Follow primary address",
        "widget_config_confirm": "Confirm",
        "settings_close": "Close settings",
        "settings_show_other_outages": "Show other outages list for other locations",
        "settings_filter_by_house_no": "Experimental: Filter by house no.",
        "err_no_internet": "No internet connection. Please check your network.",
        "msg_offline": "No internet - showing cached data:",
        "msg_using_cache": "Data cached:",
        "btn_collapse_expand_all": "Collapse/Expand All",
        "toast_slow_fetching_warning": "Fetching outages from this provider may take longer.",
        "toast_house_no_filter_warning": "Warning: Filtering by house number might skip some outages if the provider's description is incorrect.",
        "refresh_progress_prefix": "Refreshing",
        "refreshing_providers": "Refreshing providers",
        "aria_address_filter": "Filter by address",
        "aria_settings_btn": "Open settings",
        "aria_close_settings_btn": "Close settings",
        "aria_refresh_btn": "Refresh outages",
        "aria_cappucino_support": "Buy me a virtual coffee",
        "aria_github_link": "GitHub repository",
        "aria_category_toggle": "Toggle utility category"
    },
    pl: {
        "title": "Awaria",
        "settings_appearance": "Ustawienia ogólne",
        "notify": "Powiadomienia",
        "source_name": "Dostawca",
        "settings_language": "Język",
        "settings": "Ustawienia",
        "lang_system": "Domyślny systemowy",
        "lang_en": "English",
        "lang_pl": "Polski",
        "settings_theme": "Motyw",
        "theme_system": "Domyślny systemowy",
        "theme_light": "Jasny",
        "theme_dark": "Ciemny",
        "theme_emerald": "Szmaragdowy (Jasny)",
        "theme_ocean": "Oceaniczny (Ciemny)",
        "theme_nord": "Nord (Arktyczny Ciemny)",
        "theme_dracula": "Dracula (Ciemny)",
        "theme_high_contrast": "Wysoki Kontrast (AAA)",
        "theme_sepia": "Sepia (Ciepły Jasny)",
        "theme_latte": "Latte (Pastelowy Jasny)",
        "theme_monochrome_light": "Monochromatyczny (Jasny)",
        "theme_monochrome_dark": "Monochromatyczny (Ciemny)",
        "settings_font_size": "Rozmiar czcionki",
        "font_size_small": "Mały",
        "font_size_medium": "Średni",
        "font_size_large": "Duży",
        "settings_location": "Ustawienia lokalizacji",
        "settings_city": "Miasto",
        "settings_city_placeholder": "np. Wrocław",
        "settings_street": "Ulica",
        "settings_street_placeholder": "np. Kuźnicza",
        "settings_house": "Nr domu",
        "settings_house_placeholder": "np. 25 (domyślnie 1)",
        "settings_save": "Zapisz",
        "refresh_pull": "↻ Puść, aby odświeżyć",
        "loading_data": "Ładowanie danych o wyłączeniach i awariach...",
        "last_updated": "Ostatnia aktualizacja",
        "checking_updates": "Sprawdzanie aktualizacji...",
        "not_configured": "Skonfiguruj ustawienia",
        "setup_prompt": "Kliknij ⚙️ aby skonfigurować lokalizację.",
        "err_fields_required": "⚠️ Wszystkie pola są wymagane.",
        "hint_select_from_list": "Wybierz pozycję z listy.",
        "msg_looking_city": "🔍 Wyszukiwanie miasta...",
        "msg_looking_street": "🔍 Wyszukiwanie ulicy...",
        "msg_saving": "💾 Zapisywanie...",
        "msg_saved": "✅ Zapisano!",
        "err_city_not_found": "❌ Nie znaleziono miasta. Czy chodziło ci o: ",
        "err_street_not_found": "❌ Nie znaleziono ulicy. Czy chodziło ci o: ",
        "err_load_failed": "Nie udało się pobrać danych o wyłączeniach i awariach. Błąd: ",
        "lbl_your_location": "Twoja lokalizacja",
        "lbl_other_outages": "Pozostałe wyłączenia",
        "msg_no_outages_local": "Brak alertów dla twojej lokalizacji.",
        "lbl_planned_outage": "Planowane wyłączenie",

        "msg_no_alerts": "Brak aktywnych alertów.",
        "all_clear_title": "Wszystko gra!",
        "all_clear_subtitle": "Nie wykryto żadnych wyłączeń i awarii w monitorowanych obszarach.",
        "monitored_providers": "Monitorowani dostawcy",
        "refresh_now": "Odśwież teraz",
        "settings_sources": "Monitorowane media",
        "source_power": "Prąd",
        "source_heating": "Ciepło",
        "source_gas": "Gaz",
        "source_tauron_name": "Tauron",
        "source_fortum_name": "Fortum",
        "source_energa_name": "Energa",
        "source_enea_name": "Enea",
        "source_pge_name": "PGE",
        "source_water_name": "Woda",
        "source_stoen_name": "Stoen",
        "source_psg_name": "PSG",
        "source_wmk_name": "WMK Kraków",
        "source_aquanet_name": "Aquanet",
        "source_tauron_heat_name": "Tauron Ciepło",
        "source_veolia_warszawa_name": "Veolia Warszawa",
        "source_veolia_poznan_name": "Veolia Poznań",
        "source_veolia_lodz_name": "Veolia Łódź",
        "source_zwik_lodz_name": "ZWIK Łódź",
        "source_wodociagi_plockie_name": "Wodociągi Płockie",
        "source_katowickie_wodociagi_name": "Katowickie Wodociągi",
        "source_pwik_kalisz_name": "PWiK Kalisz",
        "source_pwik_czestochowa_name": "PWiK Częstochowa",
        "source_gdanskie_wodociagi_name": "Gdańskie Wodociągi",
        "source_gpec_name": "GPEC Gdańsk",
        "source_puk_rokietnica_name": "PUK Rokietnica",
        "source_tauron_short": "Tauron",
        "source_mpwik_wroclaw_name": "MPWiK Wrocław",
        "source_mpwik_wroclaw_short": "MPWiK WRO",
        "source_mpwik_warszawa_name": "MPWiK Warszawa",
        "source_mpwik_warszawa_short": "MPWiK WAW",
        "source_fortum_short": "Fortum",
        "source_energa_short": "Energa",
        "source_enea_short": "Enea",
        "source_stoen_short": "Stoen",
        "source_pge_short": "PGE",
        "source_psg_short": "PSG",
        "source_wmk_short": "WMK",
        "source_aquanet_short": "Aquanet",
        "source_tauron_heat_short": "Tauron Ciepło",
        "source_veolia_warszawa_short": "Veolia Warszawa",
        "source_veolia_poznan_short": "Veolia Poznań",
        "source_veolia_lodz_short": "Veolia Łódź",
        "source_zwik_lodz_short": "ZWIK Łódź",
        "source_wodociagi_plockie_short": "Wodociągi Płockie",
        "source_katowickie_wodociagi_short": "Kat. Wodociągi",
        "source_pwik_kalisz_short": "PWiK Kalisz",
        "source_pwik_czestochowa_short": "PWiK Częstochowa",
        "source_gdanskie_wodociagi_short": "Gdańsk Woda",
        "source_gpec_short": "GPEC",
        "source_puk_rokietnica_short": "PUK Rokietnica",
        "lbl_other_alerts_divider": "Inne lokalizacje",
        "addr_filter_all": "Wszystkie adresy",
        "lbl_address_active": "Aktywny",
        "lbl_address_disabled": "Wyłączony",
        "default_address_name": "Adres",
        "address_name": "Nazwa",
        "address_name_placeholder": "np. Dom, Praca",
        "no_addresses": "Brak skonfigurowanych adresów. Dodaj poniżej.",
        "no_streets": "Brak ulic",
        "edit_address": "Edytuj adres",
        "save_changes": "Zapisz zmiany",
        "cancel": "Anuluj",
        "footer_copyright": "© %YEAR% <a href=\"https://eremef.xyz\" target=\"_blank\" rel=\"noopener noreferrer\" class=\"external-link\">eremef</a>",
        "footer_version": "Awaria V%VERSION%",
        "footer_github": "Źródła na GitHub",
        "cuplink_support": "Podoba Ci się ta aplikacja? Postaw mi wirtualną kawę, aby wesprzeć jej rozwój!",
        "empty_state_title": "Witaj w Awarii",
        "empty_state_subtitle": "Zacznij od dodania pierwszej lokalizacji, aby monitorować przerwy w dostawie prądu, wody i ciepła.",
        "empty_state_cta": "Dodaj adres",
        "disabled_state_title": "Monitoring wstrzymany",
        "disabled_state_subtitle": "Wszystkie Twoje lokalizacje są obecnie wyłączone. Włącz je w ustawieniach, aby zobaczyć wyłączenia i awarie.",
        "disabled_state_cta": "Otwórz ustawienia",
        "sources_disabled_state_title": "Alerty wyłączone",
        "sources_disabled_state_subtitle": "Brak włączonych źródeł alertów. Włącz je w ustawieniach, aby zobaczyć wyłączenia i awarie.",
        "settings_upcoming_title": "Nadchodzące wyłączenia - dodatkowe powiadomienia",
        "settings_upcoming_prefix": "Powiadamiaj",
        "settings_upcoming_suffix": "h przed startem wyłączenia",
        "settings_notification_permission_warning": "Powiadomienia systemowe są wyłączone. Włącz je w ustawieniach Androida, aby otrzymywać powiadomienia.",
        "settings_battery_optimization_warning": "Optymalizacja baterii jest włączona. Ustaw aplikację jako „Nieograniczoną” w ustawieniach systemu Android, aby zapewnić niezawodne działanie w tle.",
        "settings_backup": "Kopia zapasowa i przywracanie",
        "settings_export": "Eksportuj ustawienia",
        "settings_import": "Importuj ustawienia",
        "msg_export_success": "Ustawienia wyeksportowane pomyślnie!",
        "msg_import_success": "Ustawienia zaimportowane pomyślnie! Odświeżanie...",
        "err_export_failed": "Nie udało się wyeksportować ustawień.",
        "err_import_failed": "Nie udało się zaimportować ustawień. Nieprawidłowy plik.",
        "widget_config_title": "Wybierz adres dla widżetu",
        "widget_config_primary": "Używaj adresu głównego",
        "widget_config_confirm": "Zatwierdź",
        "settings_close": "Zamknij ustawienia",
        "settings_show_other_outages": "Pokaż listę pozostałych wyłączeń dla innych lokalizacji",
        "settings_filter_by_house_no": "Opcja eksperymentalna: Filtruj wg nr budynków",
        "err_no_internet": "Brak połączenia z internetem. Sprawdź swoje połączenie sieciowe.",
        "msg_offline": "Brak internetu - wyświetlam informacje z:",
        "msg_using_cache": "Dane pobrane:",
        "btn_collapse_expand_all": "Zwiń/Rozwiń wszystko",
        "toast_slow_fetching_warning": "Aktualizowanie danych u tego dostawcy może potrwać chwilę dłużej:",
        "toast_house_no_filter_warning": "Uwaga: Filtrowanie po numerach budynków może pomijać niektóre zdarzenia przy nieprawidłowych danych dostawcy.",
        "refresh_progress_prefix": "Pobieranie",
        "refreshing_providers": "Aktualizowanie dostawców",
        "aria_address_filter": "Filtruj według adresu",
        "aria_settings_btn": "Otwórz ustawienia",
        "aria_close_settings_btn": "Zamknij ustawienia",
        "aria_refresh_btn": "Odśwież wyłączenia",
        "aria_cappucino_support": "Postaw mi wirtualną kawę",
        "aria_github_link": "Repozytorium GitHub",
        "aria_category_toggle": "Przełącz kategorię mediów"
    }
};

let currentLang = 'en';

/**
 * Initialize language based on saved settings or system default.
 */
function initLanguage(savedLang) {
    if (savedLang && ['en', 'pl'].includes(savedLang)) {
        currentLang = savedLang;
    } else {
        // Fallback to system language
        const sysLang = navigator.language || navigator.userLanguage;
        if (sysLang.startsWith('pl')) {
            currentLang = 'pl';
        } else {
            currentLang = 'en';
        }
    }
    applyTranslations();
}

/**
 * Translate a key.
 */
function t(key) {
    if (translations[currentLang] && translations[currentLang][key]) {
        return translations[currentLang][key];
    }
    // Fallback to english or raw key
    if (translations['en'] && translations['en'][key]) {
        return translations['en'][key];
    }
    return key;
}

/**
 * Apply translations to all elements with data-i18n attribute.
 */
function applyTranslations() {
    document.documentElement.lang = currentLang;
    const elements = document.querySelectorAll('[data-i18n], [data-i18n-title], [data-i18n-aria-label]');
    elements.forEach(el => {
        const key = el.getAttribute('data-i18n');
        const titleKey = el.getAttribute('data-i18n-title');
        const ariaLabelKey = el.getAttribute('data-i18n-aria-label');

        if (key) {
            // Handle input placeholders specifically
            if (el.tagName === 'INPUT' && el.hasAttribute('placeholder')) {
                el.setAttribute('placeholder', t(key));
            } else {
                let val = t(key);
                if (key === 'footer_copyright') {
                    const year = new Date().getFullYear();
                    el.textContent = `© ${year} `;
                    const a = document.createElement('a');
                    a.href = "https://eremef.xyz";
                    a.target = "_blank";
                    a.rel = "noopener noreferrer";
                    a.className = "external-link";
                    a.textContent = "eremef";
                    el.appendChild(a);
                } else if (key === 'footer_version') {
                    const version = window.appVersion || 'v1.0.20';
                    el.textContent = val.replace('%VERSION%', version);
                } else {
                    el.textContent = val;
                }
            }
        }

        if (titleKey) {
            el.setAttribute('title', t(titleKey));
        }

        if (ariaLabelKey) {
            el.setAttribute('aria-label', t(ariaLabelKey));
        }
    });
}

/**
 * Get the current active language string (e.g. for date formatting)
 */
function getLocaleString() {
    return currentLang === 'pl' ? 'pl-PL' : 'en-US';
}

// Export for tests
if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        translations,
        t,
        initLanguage,
        applyTranslations,
        getLocaleString,
        setCurrentLang: (lang) => { currentLang = lang; }
    };
}
