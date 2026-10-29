#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const catalogPath = path.join(root, 'providers', 'catalog.json');
const schemaPath = path.join(root, 'providers', 'catalog.schema.json');
const locales = ['pl', 'en'];
const utilities = ['power', 'gas', 'heating', 'water'];
const args = process.argv.slice(2);
const allowedArgs = new Set(['--check', '--validate', '--self-test']);
if (args.some((arg) => !allowedArgs.has(arg)) || args.length > 1) {
    throw new Error('Usage: node scripts/generate-provider-catalog.mjs [--check|--validate|--self-test]');
}

const outputs = new Map();

function read(relativePath) {
    return fs.readFileSync(path.join(root, relativePath), 'utf8');
}

function fail(where, message) {
    throw new Error(`${where}: ${message}`);
}

function assertObject(value, where, required, optional = []) {
    if (value === null || Array.isArray(value) || typeof value !== 'object') fail(where, 'expected an object');
    const allowed = new Set([...required, ...optional]);
    for (const key of Object.keys(value)) if (!allowed.has(key)) fail(`${where}.${key}`, 'unknown field');
    for (const key of required) if (!Object.hasOwn(value, key)) fail(`${where}.${key}`, 'required field is missing');
}

function assertString(value, where, { pattern, nonEmpty = true } = {}) {
    if (typeof value !== 'string' || (nonEmpty && value.length === 0)) fail(where, 'expected a non-empty string');
    if (pattern && !pattern.test(value)) fail(where, `must match ${pattern}`);
}

function assertBoolean(value, where) {
    if (typeof value !== 'boolean') fail(where, 'expected a boolean');
}

function assertOrder(value, where, nullable = false) {
    if (nullable && value === null) return;
    if (!Number.isSafeInteger(value) || value < 0 || value > 65535) fail(where, 'expected an integer from 0 to 65535');
}

function assertLocaleMap(value, where, expectedLocales = locales) {
    assertObject(value, where, expectedLocales);
    for (const locale of expectedLocales) assertString(value[locale], `${where}.${locale}`);
}

function assertStringArray(value, where, pattern) {
    if (!Array.isArray(value)) fail(where, 'expected an array');
    value.forEach((item, index) => assertString(item, `${where}[${index}]`, pattern ? { pattern } : {}));
    if (new Set(value).size !== value.length) fail(where, 'contains duplicate values');
}

function getFrontendRegionMatcherNames() {
    const frontendPath = path.join(root, 'public', 'script.js');
    const originalWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
    const originalDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
    const originalProviderCatalog = Object.getOwnPropertyDescriptor(globalThis, 'AwariaProviderCatalog');

    try {
        // script.js exposes its runtime matcher map through its existing CommonJS
        // test API. Stub only the browser globals used during module initialization.
        globalThis.window = {
            __TAURI__: { event: { listen: () => undefined } }
        };
        globalThis.document = { addEventListener: () => undefined };
        // script.js builds SOURCES at module load. An empty catalog is enough
        // for reading its matcher map and avoids depending on generated outputs.
        globalThis.AwariaProviderCatalog = { providers: [] };

        const frontend = require(frontendPath);
        const matchers = frontend.getRegionMatchers?.();
        if (!matchers || typeof matchers !== 'object' || Array.isArray(matchers)) {
            fail('public/script.js', 'getRegionMatchers() did not return a matcher map');
        }
        return new Set(Object.keys(matchers));
    } finally {
        if (originalWindow) Object.defineProperty(globalThis, 'window', originalWindow);
        else delete globalThis.window;
        if (originalDocument) Object.defineProperty(globalThis, 'document', originalDocument);
        else delete globalThis.document;
        if (originalProviderCatalog) Object.defineProperty(globalThis, 'AwariaProviderCatalog', originalProviderCatalog);
        else delete globalThis.AwariaProviderCatalog;
    }
}

function validateCatalog(catalog) {
    assertObject(catalog, 'catalog', ['schemaVersion', 'locales', 'providers', 'androidLegacyWidgets']);
    if (catalog.schemaVersion !== 1) fail('catalog.schemaVersion', 'must be 1');
    if (JSON.stringify(catalog.locales) !== JSON.stringify(locales)) fail('catalog.locales', `must be ${JSON.stringify(locales)}`);
    if (!Array.isArray(catalog.providers) || catalog.providers.length === 0) fail('catalog.providers', 'expected a non-empty array');
    if (!Array.isArray(catalog.androidLegacyWidgets)) fail('catalog.androidLegacyWidgets', 'expected an array');

    const ids = new Set();
    const idAliases = new Map();
    const androidComponents = new Set();
    const androidResources = new Set();
    const frontendRegionMatchers = getFrontendRegionMatcherNames();
    const perOrder = new Map(['frontend', 'rust', 'androidSingle', 'addon'].map((key) => [key, []]));
    const aggregateOrders = new Map(utilities.map((utility) => [utility, []]));

    catalog.providers.forEach((provider, index) => {
        const where = `providers[${index}]`;
        assertObject(provider, where, ['id', 'utility', 'order', 'defaultNotify', 'labels', 'aliases', 'bindings', 'capabilities', 'icsUtility', 'android', 'card'], ['frontend']);
        assertString(provider.id, `${where}.id`, { pattern: /^[a-z][a-z0-9_]*$/ });
        if (ids.has(provider.id)) fail(`${where}.id`, `duplicate canonical ID ${provider.id}`);
        ids.add(provider.id);
        if (!utilities.includes(provider.utility)) fail(`${where}.utility`, `must be one of ${utilities.join(', ')}`);
        if (!utilities.includes(provider.icsUtility)) fail(`${where}.icsUtility`, `must be one of ${utilities.join(', ')}`);
        assertBoolean(provider.defaultNotify, `${where}.defaultNotify`);

        assertObject(provider.order, `${where}.order`, ['frontend', 'rust', 'androidSingle', 'addon']);
        for (const key of ['frontend', 'rust', 'androidSingle']) {
            assertOrder(provider.order[key], `${where}.order.${key}`);
            perOrder.get(key).push([provider.order[key], provider.id]);
        }
        assertOrder(provider.order.addon, `${where}.order.addon`, true);
        if (provider.order.addon !== null) perOrder.get('addon').push([provider.order.addon, provider.id]);

        assertObject(provider.labels, `${where}.labels`, ['frontend', 'rust', 'android']);
        assertObject(provider.labels.frontend, `${where}.labels.frontend`, ['name', 'shortName']);
        assertLocaleMap(provider.labels.frontend.name, `${where}.labels.frontend.name`);
        assertLocaleMap(provider.labels.frontend.shortName, `${where}.labels.frontend.shortName`);
        assertObject(provider.labels.rust, `${where}.labels.rust`, ['name']);
        assertLocaleMap(provider.labels.rust.name, `${where}.labels.rust.name`, ['pl']);
        assertObject(provider.labels.android, `${where}.labels.android`, ['shortName', 'legacyWidget']);
        assertLocaleMap(provider.labels.android.shortName, `${where}.labels.android.shortName`);
        assertObject(provider.labels.android.legacyWidget, `${where}.labels.android.legacyWidget`, ['resource', 'name']);
        assertString(provider.labels.android.legacyWidget.resource, `${where}.labels.android.legacyWidget.resource`, { pattern: /^[a-z][a-z0-9_]*$/ });
        assertLocaleMap(provider.labels.android.legacyWidget.name, `${where}.labels.android.legacyWidget.name`);

        assertStringArray(provider.aliases, `${where}.aliases`, /^[a-z][a-z0-9_]*$/);
        for (const alias of provider.aliases) {
            if (ids.has(alias)) fail(`${where}.aliases`, `alias ${alias} collides with a canonical ID`);
            if (idAliases.has(alias)) fail(`${where}.aliases`, `alias ${alias} is already owned by ${idAliases.get(alias)}`);
            idAliases.set(alias, provider.id);
        }

        assertObject(provider.bindings, `${where}.bindings`, ['rust', 'android']);
        assertObject(provider.bindings.rust, `${where}.bindings.rust`, ['enumVariant', 'constructor']);
        assertString(provider.bindings.rust.enumVariant, `${where}.bindings.rust.enumVariant`, { pattern: /^[A-Z][A-Za-z0-9]*$/ });
        assertString(provider.bindings.rust.constructor, `${where}.bindings.rust.constructor`, { pattern: /^[a-z][a-z0-9_]*::[A-Z][A-Za-z0-9]*$/ });
        assertObject(provider.bindings.android, `${where}.bindings.android`, ['legacyComponentClass', 'legacyLabelResource']);
        assertString(provider.bindings.android.legacyComponentClass, `${where}.bindings.android.legacyComponentClass`, { pattern: /^[A-Za-z][A-Za-z0-9]*$/ });
        assertString(provider.bindings.android.legacyLabelResource, `${where}.bindings.android.legacyLabelResource`, { pattern: /^[a-z][a-z0-9_]*$/ });
        if (androidComponents.has(provider.bindings.android.legacyComponentClass)) fail(`${where}.bindings.android.legacyComponentClass`, 'duplicate Android component binding');
        androidComponents.add(provider.bindings.android.legacyComponentClass);

        assertObject(provider.capabilities, `${where}.capabilities`, ['webviewOnly', 'addonAllowed']);
        assertBoolean(provider.capabilities.webviewOnly, `${where}.capabilities.webviewOnly`);
        assertBoolean(provider.capabilities.addonAllowed, `${where}.capabilities.addonAllowed`);
        if (provider.capabilities.addonAllowed !== (provider.order.addon !== null)) fail(`${where}.capabilities.addonAllowed`, 'must agree with order.addon');

        if (provider.frontend !== undefined) {
            assertObject(provider.frontend, `${where}.frontend`, ['regionMatcher']);
            assertString(provider.frontend.regionMatcher, `${where}.frontend.regionMatcher`, { pattern: /^[a-z][a-z0-9_]*$/ });
            if (!frontendRegionMatchers.has(provider.frontend.regionMatcher)) {
                fail(`${where}.frontend.regionMatcher`, `unknown frontend matcher ${provider.frontend.regionMatcher}`);
            }
        }

        assertObject(provider.android, `${where}.android`, ['singleWidget', 'searchAliases', 'location', 'aggregateUtility', 'aggregateOrder']);
        assertBoolean(provider.android.singleWidget, `${where}.android.singleWidget`);
        assertStringArray(provider.android.searchAliases, `${where}.android.searchAliases`);
        assertString(provider.android.location, `${where}.android.location`, { nonEmpty: false });
        if (provider.android.singleWidget !== (provider.order.androidSingle !== null)) fail(`${where}.android.singleWidget`, 'must agree with order.androidSingle');
        if (provider.android.aggregateUtility !== null && !utilities.includes(provider.android.aggregateUtility)) fail(`${where}.android.aggregateUtility`, `must be null or one of ${utilities.join(', ')}`);
        assertOrder(provider.android.aggregateOrder, `${where}.android.aggregateOrder`, true);
        if ((provider.android.aggregateUtility === null) !== (provider.android.aggregateOrder === null)) fail(`${where}.android`, 'aggregateUtility and aggregateOrder must both be set or both be null');
        if (provider.android.aggregateUtility !== null) {
            if (provider.android.aggregateUtility !== provider.utility) fail(`${where}.android.aggregateUtility`, 'must match provider utility');
            aggregateOrders.get(provider.android.aggregateUtility).push([provider.android.aggregateOrder, provider.id]);
        }

        assertObject(provider.card, `${where}.card`, ['name', 'icon', 'color']);
        assertString(provider.card.name, `${where}.card.name`);
        assertString(provider.card.icon, `${where}.card.icon`);
        assertString(provider.card.color, `${where}.card.color`, { pattern: /^#[0-9a-fA-F]{6}$/ });

        const legacyResource = provider.labels.android.legacyWidget.resource;
        if (legacyResource !== provider.bindings.android.legacyLabelResource) fail(`${where}.bindings.android.legacyLabelResource`, 'must match labels.android.legacyWidget.resource');
        if (androidResources.has(legacyResource)) fail(`${where}.labels.android.legacyWidget.resource`, 'duplicate Android string resource');
        androidResources.add(legacyResource);
    });

    const assertDense = (rows, label) => {
        rows.sort((left, right) => left[0] - right[0]);
        rows.forEach(([order], index) => {
            if (order !== index) fail(`providers.order.${label}`, `expected unique dense ordering; got ${order} at position ${index}`);
        });
    };
    for (const [key, values] of perOrder) assertDense(values, key);
    for (const [utility, values] of aggregateOrders) assertDense(values, `android.${utility}.aggregateOrder`);
    for (const alias of idAliases.keys()) if (ids.has(alias)) fail('providers.aliases', `alias ${alias} collides with a canonical ID`);

    const legacyComponents = new Set(androidComponents);
    catalog.androidLegacyWidgets.forEach((widget, index) => {
        const where = `androidLegacyWidgets[${index}]`;
        assertObject(widget, where, ['componentClass', 'resource', 'name']);
        assertString(widget.componentClass, `${where}.componentClass`, { pattern: /^[A-Za-z][A-Za-z0-9]*$/ });
        assertString(widget.resource, `${where}.resource`, { pattern: /^[a-z][a-z0-9_]*$/ });
        assertLocaleMap(widget.name, `${where}.name`);
        if (legacyComponents.has(widget.componentClass)) fail(`${where}.componentClass`, 'duplicate Android component binding');
        legacyComponents.add(widget.componentClass);
        if (androidResources.has(widget.resource)) fail(`${where}.resource`, 'duplicate Android string resource');
        androidResources.add(widget.resource);
    });

    if (catalog.providers.some((provider, index) => provider.order.frontend !== index)) fail('providers.order.frontend', 'provider array must be in frontend display order');
    validateBindingsAgainstCode(catalog, legacyComponents);
}

function validateBindingsAgainstCode(catalog, catalogComponents) {
    const api = read('src-tauri/src/api_logic.rs');
    const lib = read('src-tauri/src/lib.rs');
    const enumBlock = api.match(/pub enum AlertSource \{([\s\S]*?)\n\}/)?.[1];
    if (!enumBlock) fail('src-tauri/src/api_logic.rs', 'could not find AlertSource enum');
    const serializedIds = new Map();
    let overrideId = null;
    for (const line of enumBlock.split(/\r?\n/)) {
        const rename = line.match(/#\[serde\(rename\s*=\s*"([^"]+)"\)\]/);
        if (rename) overrideId = rename[1];
        const variant = line.match(/^\s*([A-Z][A-Za-z0-9_]*)\s*,\s*$/)?.[1];
        if (variant) {
            serializedIds.set(variant, overrideId || variant.toLowerCase());
            overrideId = null;
        }
    }
    const expectedVariantIds = new Map(catalog.providers.map((provider) => [provider.bindings.rust.enumVariant, provider.id]));
    if (serializedIds.size !== expectedVariantIds.size) fail('bindings.rust', `AlertSource has ${serializedIds.size} variants; catalog has ${expectedVariantIds.size}`);
    for (const [variant, id] of expectedVariantIds) if (serializedIds.get(variant) !== id) fail(`bindings.rust.${variant}`, `catalog ID ${id} does not match AlertSource ID ${serializedIds.get(variant)}`);

    const constructorBlock = lib.match(/pub fn get_providers\(\)\s*->\s*Vec<Box<dyn AlertProvider>>\s*\{\s*vec!\[([\s\S]*?)\n\s*\]\s*\n\}/)?.[1];
    if (!constructorBlock) fail('src-tauri/src/lib.rs', 'could not find get_providers registry');
    const constructors = [...constructorBlock.matchAll(/Box::new\(([a-z][a-z0-9_]*::[A-Z][A-Za-z0-9]*)\)/g)].map((match) => match[1]);
    const expectedConstructors = catalog.providers.slice().sort((a, b) => a.order.rust - b.order.rust).map((provider) => provider.bindings.rust.constructor);
    if (JSON.stringify(constructors) !== JSON.stringify(expectedConstructors)) fail('bindings.rust.constructor', 'catalog order/constructor list does not match get_providers()');
    for (const provider of catalog.providers) {
        const module = provider.bindings.rust.constructor.split('::')[0];
        const moduleSource = read(`src-tauri/src/${module}.rs`);
        const sourceVariant = moduleSource.match(/fn source\s*\(\s*&self\s*\)\s*->\s*AlertSource\s*\{\s*AlertSource::([A-Za-z0-9_]+)/)?.[1];
        if (sourceVariant !== provider.bindings.rust.enumVariant) fail(`providers.${provider.id}.bindings.rust`, `implementation reports AlertSource::${sourceVariant}, expected ${provider.bindings.rust.enumVariant}`);
    }

    const manifest = read('src-tauri/gen/android/app/src/main/AndroidManifest.xml');
    const manifestComponents = new Map();
    for (const match of manifest.matchAll(/<receiver\b([\s\S]*?)<\/receiver>/g)) {
        const component = match[1].match(/android:name="([^"]+)"/)?.[1]?.split('.').at(-1);
        const resource = match[1].match(/android:label="@string\/([^"]+)"/)?.[1];
        if (component && resource) manifestComponents.set(component, resource);
    }
    if (manifestComponents.size !== catalogComponents.size) fail('bindings.android', `manifest has ${manifestComponents.size} labeled components; catalog has ${catalogComponents.size}`);
    for (const provider of catalog.providers) {
        const component = provider.bindings.android.legacyComponentClass;
        if (manifestComponents.get(component) !== provider.bindings.android.legacyLabelResource) fail(`providers.${provider.id}.bindings.android`, `manifest resource for ${component} does not match catalog`);
    }
    for (const widget of catalog.androidLegacyWidgets) {
        if (manifestComponents.get(widget.componentClass) !== widget.resource) fail(`androidLegacyWidgets.${widget.componentClass}`, 'manifest label resource does not match catalog');
    }
}

function runValidationSelfTests(catalog) {
    const cases = [
        ['unknown field', (copy) => { copy.unrecognized = true; }, 'catalog.unrecognized'],
        ['duplicate ID', (copy) => { copy.providers[1].id = copy.providers[0].id; }, 'duplicate canonical ID'],
        ['missing translation', (copy) => { delete copy.providers[0].labels.frontend.name.en; }, 'labels.frontend.name.en'],
        ['bad utility', (copy) => { copy.providers[0].utility = 'steam'; }, 'providers[0].utility'],
        ['unknown frontend matcher', (copy) => { const provider = copy.providers.find((item) => item.frontend); provider.frontend.regionMatcher = 'missing_matcher'; }, 'unknown frontend matcher missing_matcher'],
        ['duplicate ID aliases', (copy) => { copy.providers[0].aliases = ['legacy_id', 'legacy_id']; }, 'providers[0].aliases'],
        ['incompatible aggregate fields', (copy) => { copy.providers[0].android.aggregateUtility = null; }, 'aggregateUtility and aggregateOrder'],
        ['add-on membership drift', (copy) => { copy.providers[0].capabilities.addonAllowed = !copy.providers[0].capabilities.addonAllowed; }, 'must agree with order.addon']
    ];
    for (const [name, mutate, expectedError] of cases) {
        const copy = structuredClone(catalog);
        mutate(copy);
        try {
            validateCatalog(copy);
        } catch (error) {
            if (error.message.includes(expectedError)) continue;
            throw new Error(`Self-test "${name}" failed with an unexpected diagnostic: ${error.message}`);
        }
        throw new Error(`Self-test "${name}" did not reject invalid catalog data.`);
    }
    console.log(`Passed ${cases.length} intentional-invalid catalog validation cases.`);
}

function jsString(value) {
    return JSON.stringify(value);
}

function renderFrontend(catalog) {
    const providers = catalog.providers.slice().sort((a, b) => a.order.frontend - b.order.frontend).map((provider) => ({
        id: provider.id,
        utility: provider.utility,
        category: provider.utility,
        defaultNotify: provider.defaultNotify,
        frontendOrder: provider.order.frontend,
        label: provider.labels.frontend.name.pl,
        names: provider.labels.frontend.name,
        shortNames: provider.labels.frontend.shortName,
        aliases: provider.aliases,
        surfaceNames: { rust: provider.labels.rust.name, android: provider.labels.android.shortName },
        ...(provider.frontend ? { region: provider.frontend.regionMatcher } : {})
    }));
    return `/* Generated from providers/catalog.json. Do not edit. */
(function (root, factory) {
    const catalog = factory();
    if (typeof module === 'object' && module.exports) module.exports = catalog;
    else root.AwariaProviderCatalog = catalog;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    'use strict';
    const providers = ${JSON.stringify(providers, null, 4)}.map((provider) => Object.freeze(provider));
    const byId = Object.freeze(Object.fromEntries(providers.map((provider) => [provider.id, provider])));
    function getName(id, locale = 'pl', short = false, surface = 'frontend') {
        const provider = byId[id];
        if (!provider) return null;
        const names = surface === 'frontend'
            ? (short ? provider.shortNames : provider.names)
            : provider.surfaceNames?.[surface];
        return names?.[locale] || names?.pl || names?.en || null;
    }
    return Object.freeze({ providers: Object.freeze(providers), byId, getName });
});
`;
}

function renderCardCatalog(catalog) {
    const cards = catalog.providers.map((provider) => ({ id: provider.id, aliases: provider.aliases, ...provider.card }));
    return `/* Generated from providers/catalog.json. This file is standalone. */
(function (root, factory) {
    const catalog = factory();
    if (typeof module === 'object' && module.exports) module.exports = catalog;
    else root.AwariaProviderCardCatalog = catalog;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    'use strict';
    const providers = ${JSON.stringify(cards, null, 4)};
    const byId = Object.freeze(Object.fromEntries(providers.flatMap((provider) => [[provider.id, Object.freeze(provider)], ...provider.aliases.map((alias) => [alias, Object.freeze(provider)])])));
    return Object.freeze({ providers: Object.freeze(providers), byId, find: (source) => byId[(source || '').toLowerCase()] || null });
});
`;
}

function rustString(value) {
    return JSON.stringify(value).replace(/\u2028/g, '\\u{2028}').replace(/\u2029/g, '\\u{2029}');
}

function renderRust(catalog) {
    const providers = catalog.providers.slice().sort((a, b) => a.order.rust - b.order.rust);
    const utility = (value) => ({ power: 'Power', gas: 'Gas', heating: 'Heating', water: 'Water' })[value];
    const rows = providers.map((provider) => `    ProviderMetadata {
        id: ${rustString(provider.id)},
        utility: CatalogUtility::${utility(provider.utility)},
        ics_utility: CatalogUtility::${utility(provider.icsUtility)},
        frontend_order: ${provider.order.frontend},
        rust_order: ${provider.order.rust},
        android_order: ${provider.order.androidSingle},
        aggregate_utility: ${provider.android.aggregateUtility === null ? 'None' : `Some(CatalogUtility::${utility(provider.android.aggregateUtility)})`},
        aggregate_order: ${provider.android.aggregateOrder === null ? 'None' : `Some(${provider.android.aggregateOrder})`},
        addon_order: ${provider.order.addon === null ? 'None' : `Some(${provider.order.addon})`},
        default_notify: ${provider.defaultNotify},
        display_name_pl: ${rustString(provider.labels.frontend.name.pl)},
        display_name_en: ${rustString(provider.labels.frontend.name.en)},
        short_name_pl: ${rustString(provider.labels.frontend.shortName.pl)},
        short_name_en: ${rustString(provider.labels.frontend.shortName.en)},
        rust_display_name_pl: ${rustString(provider.labels.rust.name.pl)},
        enum_variant: ${rustString(provider.bindings.rust.enumVariant)},
        constructor: ${rustString(provider.bindings.rust.constructor)},
        webview_only: ${provider.capabilities.webviewOnly},
    },`).join('\n');
    return `// Generated from providers/catalog.json. Do not edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogUtility {
    Power,
    Gas,
    Heating,
    Water,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub id: &'static str,
    pub utility: CatalogUtility,
    pub ics_utility: CatalogUtility,
    pub frontend_order: u16,
    pub rust_order: u16,
    pub android_order: u16,
    pub aggregate_utility: Option<CatalogUtility>,
    pub aggregate_order: Option<u16>,
    pub addon_order: Option<u16>,
    pub default_notify: bool,
    pub display_name_pl: &'static str,
    pub display_name_en: &'static str,
    pub short_name_pl: &'static str,
    pub short_name_en: &'static str,
    pub rust_display_name_pl: &'static str,
    pub enum_variant: &'static str,
    pub constructor: &'static str,
    pub webview_only: bool,
}

pub const PROVIDERS: &[ProviderMetadata] = &[
${rows}
];
`;
}

function kotlinString(value) {
    return JSON.stringify(value).replaceAll('$', '\\$');
}

function renderKotlin(catalog) {
    const providers = catalog.providers.slice().sort((a, b) => a.order.androidSingle - b.order.androidSingle);
    const rows = providers.map((provider) => `        GeneratedProviderMetadata(
            id = ${kotlinString(provider.id)},
            utility = ${kotlinString(provider.utility)},
            displayNamePl = ${kotlinString(provider.labels.android.shortName.pl)},
            displayNameEn = ${kotlinString(provider.labels.android.shortName.en)},
            shortNamePl = ${kotlinString(provider.labels.frontend.shortName.pl)},
            shortNameEn = ${kotlinString(provider.labels.frontend.shortName.en)},
            androidOrder = ${provider.order.androidSingle},
            singleWidget = ${provider.android.singleWidget},
            legacyComponentClass = ${kotlinString(provider.bindings.android.legacyComponentClass)},
            searchAliases = listOf(${provider.android.searchAliases.map(kotlinString).join(', ')}),
            location = ${kotlinString(provider.android.location)},
            aggregateUtility = ${provider.android.aggregateUtility === null ? 'null' : kotlinString(provider.android.aggregateUtility)},
            aggregateOrder = ${provider.android.aggregateOrder === null ? 'null' : provider.android.aggregateOrder},
            addonAllowed = ${provider.capabilities.addonAllowed},
            webviewOnly = ${provider.capabilities.webviewOnly}
        )`).join(',\n');
    return `// Generated from providers/catalog.json. Do not edit.
package xyz.eremef.awaria

data class GeneratedProviderMetadata(
    val id: String,
    val utility: String,
    val displayNamePl: String,
    val displayNameEn: String,
    val shortNamePl: String,
    val shortNameEn: String,
    val androidOrder: Int?,
    val singleWidget: Boolean,
    val legacyComponentClass: String?,
    val searchAliases: List<String>,
    val location: String?,
    val aggregateUtility: String?,
    val aggregateOrder: Int?,
    val addonAllowed: Boolean,
    val webviewOnly: Boolean
)

object GeneratedProviderCatalog {
    val entries: List<GeneratedProviderMetadata> = listOf(
${rows}
    )

    val byId: Map<String, GeneratedProviderMetadata> = entries.associateBy { it.id }

    fun find(id: String?): GeneratedProviderMetadata? = id?.let(byId::get)

    fun aggregate(utility: String): List<GeneratedProviderMetadata> = entries
        .filter { it.aggregateUtility == utility }
        .sortedBy { it.aggregateOrder ?: Int.MAX_VALUE }
}
`;
}

function xmlEscape(value) {
    return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

function renderAndroidStrings(catalog, locale) {
    const labels = [];
    for (const provider of catalog.providers) labels.push([provider.labels.android.legacyWidget.resource, provider.labels.android.legacyWidget.name[locale]]);
    for (const widget of catalog.androidLegacyWidgets) {
        if (widget.resource.startsWith('widget_label_')) labels.push([widget.resource, widget.name[locale]]);
    }
    labels.sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
    return `<?xml version="1.0" encoding="utf-8"?>
<!-- Generated from providers/catalog.json. Do not edit. -->
<resources>
${labels.map(([key, value]) => `    <string name="${key}">${xmlEscape(value)}</string>`).join('\n')}
</resources>
`;
}

function renderAddonSources(catalog) {
    const allowed = catalog.providers.filter((provider) => provider.capabilities.addonAllowed).sort((a, b) => a.order.addon - b.order.addon);
    return `# Generated from providers/catalog.json. Do not edit.
allowed_sources:
${allowed.map((provider) => `  - ${provider.id}`).join('\n')}
`;
}

function addOutput(relativePath, content) {
    outputs.set(relativePath, content.replace(/\r\n/g, '\n'));
}

function renderOutputs(catalog) {
    addOutput('public/provider-catalog.generated.js', renderFrontend(catalog));
    addOutput('public/provider-card-catalog.generated.js', renderCardCatalog(catalog));
    const cardPath = 'public/awaria-card.js';
    const card = read(cardPath).replace(/\r\n/g, '\n');
    const start = '// BEGIN GENERATED PROVIDER CARD CATALOG';
    const end = '// END GENERATED PROVIDER CARD CATALOG';
    const beginAt = card.indexOf(start);
    const endAt = card.indexOf(end);
    if (beginAt < 0 || endAt < beginAt || card.indexOf(start, beginAt + start.length) !== -1 || card.indexOf(end, endAt + end.length) !== -1) {
        throw new Error(`${cardPath}: expected exactly one generated catalog marker pair`);
    }
    const embedded = renderCardCatalog(catalog).replace(
        "    if (typeof module === 'object' && module.exports) module.exports = catalog;\n    else root.AwariaProviderCardCatalog = catalog;",
        '    root.AwariaProviderCardCatalog = catalog;'
    ).trimEnd();
    addOutput(cardPath, card.slice(0, beginAt) + start + '\n' + embedded + '\n' + card.slice(endAt));
    addOutput('src-tauri/src/provider_catalog.generated.rs', renderRust(catalog));
    addOutput('src-tauri/gen/android/app/src/main/java/xyz/eremef/awaria/ProviderCatalog.generated.kt', renderKotlin(catalog));
    addOutput('src-tauri/gen/android/app/src/main/res/values/provider_catalog.generated.xml', renderAndroidStrings(catalog, 'pl'));
    addOutput('src-tauri/gen/android/app/src/main/res/values-en/provider_catalog.generated.xml', renderAndroidStrings(catalog, 'en'));
    addOutput('awaria_outage_monitor/provider-sources.generated.yaml', renderAddonSources(catalog));
    const configPath = 'awaria_outage_monitor/config.yaml';
    const config = read(configPath).replace(/\r\n/g, '\n');
    const schemaEntry = /(^schema:\n[\s\S]*?\n  enabled_sources:\n    - list\()[^\n)]*(\))/m;
    if (!schemaEntry.test(config)) throw new Error(`${configPath}: missing enabled_sources schema list`);
    const allowed = catalog.providers.filter((provider) => provider.capabilities.addonAllowed)
        .sort((a, b) => a.order.addon - b.order.addon).map((provider) => provider.id).join('|');
    addOutput(configPath, config.replace(schemaEntry, (_, before, after) => before + allowed + after));
}

const catalog = JSON.parse(read('providers/catalog.json'));
validateCatalog(catalog);
if (args[0] === '--self-test') {
    runValidationSelfTests(catalog);
    process.exit(0);
}
if (args[0] === '--validate') {
    console.log(`Validated ${catalog.providers.length} providers and ${catalog.androidLegacyWidgets.length} non-provider Android widget labels.`);
    process.exit(0);
}
renderOutputs(catalog);

let drift = false;
for (const [relativePath, expected] of outputs) {
    const absolutePath = path.join(root, relativePath);
    const actual = fs.existsSync(absolutePath) ? fs.readFileSync(absolutePath, 'utf8').replace(/\r\n/g, '\n') : null;
    if (actual === expected) continue;
    if (args[0] === '--check') {
        console.error(`${actual === null ? 'Missing generated file' : 'Stale generated file'}: ${relativePath}`);
        drift = true;
        continue;
    }
    fs.mkdirSync(path.dirname(absolutePath), { recursive: true });
    fs.writeFileSync(absolutePath, expected, 'utf8');
    console.log(`Generated ${relativePath}`);
}
if (drift) process.exitCode = 1;
