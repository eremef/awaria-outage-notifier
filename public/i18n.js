// i18n.js
// Basic translation dictionary and helper functions for frontend

const translations = {
    en: {
        "title": "Tauron Outages",
        "settings_appearance": "Appearance",
        "settings_theme": "Theme",
        "theme_system": "System Default",
        "theme_light": "Light",
        "theme_dark": "Dark",
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
        "settings_save": "Save & Lookup",
        "refresh_pull": "↻ Release to refresh",
        "refresh_loading": "↻ Refreshing...",
        "loading_data": "Loading outage data...",
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
        "err_load_failed": "Failed to load outage data. Error: ",
        "lbl_your_location": "Your location",
        "lbl_other_outages": "Other outages",
        "msg_no_outages_local": "No planned outages for your location.",
        "lbl_planned_outage": "Planned Outage"
    },
    pl: {
        "title": "Tauron - Wyłączenia",
        "settings_appearance": "Wygląd",
        "settings_theme": "Motyw",
        "theme_system": "Domyślny systemowy",
        "theme_light": "Jasny",
        "theme_dark": "Ciemny",
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
        "settings_save": "Zapisz i szukaj",
        "refresh_pull": "↻ Puść, aby odświeżyć",
        "refresh_loading": "↻ Odświeżanie...",
        "loading_data": "Ładowanie danych o wyłączeniach...",
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
        "err_load_failed": "Nie udało się pobrać danych o wyłączeniach. Błąd: ",
        "lbl_your_location": "Twoja lokalizacja",
        "lbl_other_outages": "Pozostałe wyłączenia",
        "msg_no_outages_local": "Brak planowanych wyłączeń dla twojej lokalizacji.",
        "lbl_planned_outage": "Planowane wyłączenie"
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
