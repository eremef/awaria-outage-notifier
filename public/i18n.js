// i18n.js
// Basic translation dictionary and helper functions for frontend

const translations = {
    en: {
        "title": "Awaria",
        "settings_appearance": "Appearance",
        "settings_theme": "Theme",
        "theme_system": "System Default",
        "theme_light": "Light",
        "theme_dark": "Dark",
        "theme_emerald": "Emerald (Light)",
        "theme_ocean": "Ocean (Dark)",
        "theme_nord": "Nord (Arctic Dark)",
        "theme_dracula": "Dracula (Vampire)",
        "theme_sepia": "Sepia (Warm Light)",
        "theme_latte": "Latte (Pastel Light)",
        "theme_monochrome_light": "Monochrome (Light)",
        "theme_monochrome_dark": "Monochrome (Dark)",
        "settings_location": "Location Settings",
        "settings_language": "Language",
        "lang_system": "System Default",
        "lang_en": "English",
        "lang_pl": "Polski",
        "settings_city": "City",
        "settings_city_placeholder": "e.g. Wrocław",
        "settings_street": "Street",
        "settings_street_placeholder": "e.g. Kuźnicza",
        "settings_house": "House No",
        "settings_house_placeholder": "e.g. 25",
        "settings_save": "Save",
        "refresh_pull": "↻ Release to refresh",
        "refresh_loading": "↻ Refreshing...",
        "loading_data": "Loading alert data...",
        "last_updated": "Last updated",
        "checking_updates": "Checking for updates...",
        "not_configured": "Not configured",
        "setup_prompt": "Tap ⚙️ to configure your location.",
        "err_fields_required": "⚠️ All fields are required.",
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
        "source_tauron": "⚡ Power Outage",
        "source_water": "💧 Water Outage",
        "source_fortum": "🔥 Heating Outage (Fortum)",
        "source_energa": "⚡ Energa Outage",
        "source_enea": "⚡ Enea Outage",
        "lbl_section_tauron": "Power (Tauron)",
        "lbl_section_water": "Water (MPWiK)",
        "lbl_section_fortum": "Heating (Fortum)",
        "lbl_section_energa": "Power (Energa)",
        "lbl_section_enea": "Power (Enea)",
        "msg_no_alerts": "No active alerts.",
        "settings_sources": "Alert Sources",
        "source_power": "Power",
        "source_heating": "Heating",
        "source_tauron_name": "Tauron",
        "source_fortum_name": "Fortum",
        "source_energa_name": "Energa",
        "source_enea_name": "Enea",
        "source_water_name": "Water",
        "source_tauron_short": "Tauron",
        "source_water_short": "MPWiK",
        "source_fortum_short": "Fortum",
        "source_energa_short": "Energa",
        "source_enea_short": "Enea",
        "lbl_other_alerts_divider": "Other alerts",
        "addr_filter_all": "All addresses",
        "add_address": "+ Add Address",
        "address_name": "Name",
        "address_name_placeholder": "e.g. Home, Work",
        "no_addresses": "No addresses configured. Add one below.",
        "no_streets": "No streets",
        "edit_address": "Edit Address",
        "save_changes": "Save Changes",
        "cancel": "Cancel"
    },
    pl: {
        "title": "Awaria",
        "settings_appearance": "Wygląd",
        "settings_theme": "Motyw",
        "theme_system": "Domyślny systemowy",
        "theme_light": "Jasny",
        "theme_dark": "Ciemny",
        "theme_emerald": "Szmaragdowy (Jasny)",
        "theme_ocean": "Oceaniczny (Ciemny)",
        "theme_nord": "Nord (Arktyczny Ciemny)",
        "theme_dracula": "Dracula (Ciemny)",
        "theme_sepia": "Sepia (Ciepły Jasny)",
        "theme_latte": "Latte (Pastelowy Jasny)",
        "theme_monochrome_light": "Monochromatyczny (Jasny)",
        "theme_monochrome_dark": "Monochromatyczny (Ciemny)",
        "settings_location": "Ustawienia lokalizacji",
        "settings_language": "Język",
        "lang_system": "Domyślny systemowy",
        "lang_en": "English",
        "lang_pl": "Polski",
        "settings_city": "Miasto",
        "settings_city_placeholder": "np. Wrocław",
        "settings_street": "Ulica",
        "settings_street_placeholder": "np. Kuźnicza",
        "settings_house": "Nr domu",
        "settings_house_placeholder": "np. 25",
        "settings_save": "Zapisz",
        "refresh_pull": "↻ Puść, aby odświeżyć",
        "refresh_loading": "↻ Odświeżanie...",
        "loading_data": "Ładowanie danych o awariach...",
        "last_updated": "Ostatnia aktualizacja",
        "checking_updates": "Sprawdzanie aktualizacji...",
        "not_configured": "Skonfiguruj ustawienia",
        "setup_prompt": "Kliknij ⚙️ aby skonfigurować lokalizację.",
        "err_fields_required": "⚠️ Wszystkie pola są wymagane.",
        "msg_looking_city": "🔍 Wyszukiwanie miasta...",
        "msg_looking_street": "🔍 Wyszukiwanie ulicy...",
        "msg_saving": "💾 Zapisywanie...",
        "msg_saved": "✅ Zapisano!",
        "err_city_not_found": "❌ Nie znaleziono miasta. Czy chodziło ci o: ",
        "err_street_not_found": "❌ Nie znaleziono ulicy. Czy chodziło ci o: ",
        "err_load_failed": "Nie udało się pobrać danych o awariach. Błąd: ",
        "lbl_your_location": "Twoja lokalizacja",
        "lbl_other_outages": "Pozostałe wyłączenia",
        "msg_no_outages_local": "Brak alertów dla twojej lokalizacji.",
        "lbl_planned_outage": "Planowane wyłączenie",
        "source_tauron": "⚡ Wyłączenie prądu (Tauron)",
        "source_water": "💧 Wyłączenie wody (MPWiK)",
        "source_fortum": "🔥 Wyłączenie ogrzewania (Fortum)",
        "source_energa": "⚡ Wyłączenie prądu (Energa)",
        "source_enea": "⚡ Wyłączenie prądu (Enea)",
        "lbl_section_tauron": "Prąd (Tauron)",
        "lbl_section_water": "Woda (MPWiK)",
        "lbl_section_fortum": "Ogrzewanie (Fortum)",
        "lbl_section_energa": "Prąd (Energa)",
        "lbl_section_enea": "Prąd (Enea)",
        "msg_no_alerts": "Brak aktywnych alertów.",
        "settings_sources": "Źródła alertów",
        "source_power": "Prąd",
        "source_heating": "Ogrzewanie",
        "source_tauron_name": "Tauron",
        "source_fortum_name": "Fortum",
        "source_energa_name": "Energa",
        "source_enea_name": "Enea",
        "source_water_name": "Woda",
        "source_tauron_short": "Tauron",
        "source_water_short": "MPWiK",
        "source_fortum_short": "Fortum",
        "source_energa_short": "Energa",
        "source_enea_short": "Enea",
        "lbl_other_alerts_divider": "Inne alerty",
        "addr_filter_all": "Wszystkie adresy",
        "add_address": "+ Dodaj adres",
        "address_name": "Nazwa",
        "address_name_placeholder": "np. Dom, Praca",
        "no_addresses": "Brak skonfigurowanych adresów. Dodaj poniżej.",
        "no_streets": "Brak ulic",
        "edit_address": "Edytuj adres",
        "save_changes": "Zapisz zmiany",
        "cancel": "Anuluj"
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
    const elements = document.querySelectorAll('[data-i18n]');
    elements.forEach(el => {
        const key = el.getAttribute('data-i18n');

        // Handle input placeholders specifically
        if (el.tagName === 'INPUT' && el.hasAttribute('placeholder')) {
            // Only translate if there's a specific placeholder key, else default to textContent style
            // We use key + "_placeholder" if it exists, otherwise just the key
            const val = t(key);
            el.setAttribute('placeholder', val);
        } else {
            el.textContent = t(key);
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
