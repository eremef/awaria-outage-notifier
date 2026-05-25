# Frontend Integration (JS/CSS)

## 1. Data Schema (`public/script.js`)

Add the provider to `SOURCES`:
```javascript
{
    id: "provider_id",
    name: "Provider Name",
    category: "power", // or water, gas, heat
    i18n: "provider_i18n_key"
}
```

## 2. City Grouping

In `renderAlerts()`, update the grouping logic for "Other Outages":
1. Create `isCity(addr)` helper.
2. Update the `addresses.some(isCity)` check.

## 3. Styling & Themes (`public/style.css`)

You MUST define the brand color for EVERY theme.

### CSS Variables
```css
:root { --provider-color: #HEX; } /* Light theme default */
[data-theme="dark"] { --provider-color: #HEX; }
.emerald { --provider-color: #HEX; }
/* ... and so on for ocean, nord, dracula, sepia, latte ... */
```

### Component Rules
```css
.card.source-provider_id .outage-type { color: var(--provider-color); }
.card.source-provider_id { border-top: 4px solid var(--provider-color); }
/* Update the fallback selector */
.card:not(.source-tauron):not(.source-provider_id) { border-top: 4px solid #7c7c7c; }

/* Settings label color */
.collapsible.source-provider_id .section-label.other { color: var(--provider-color); }
```

## 4. Translations (`public/i18n.js`)

Add entries for the provider name and any specific status labels.
