# Provider catalog

`catalog.json` is the version-controlled source for provider identity, shared labels, utilities, defaults, surface ordering, and explicit consumer bindings. Runtime code still owns provider fetching, service-area predicates, source-specific matching, and other executable behavior.

## Schema and maintenance

- IDs are stable lowercase serialized source IDs. Keep them unchanged; add a catalog alias when an old external ID must remain recognized.
- `utility` uses `power`, `gas`, `heating`, or `water`. It is shared metadata, not a service-area predicate.
- `labels` records each surface's current names. Frontend names and short names are required in Polish and English. `labels.android.shortName` contains the original localized Android `provider_*` strings used in widgets; `legacyWidget.name` is the separate launcher label. Rust and Android labels are explicit because they intentionally differ by surface.
- `order.frontend`, `order.rust`, and `order.androidSingle` preserve independent user-visible and runtime orderings. `order.addon` is the position in the add-on allowlist, or `null`; `capabilities.addonAllowed` must agree with it.
- `android.aggregateUtility` and `android.aggregateOrder` are set together only when the source is included in All/Tri widgets. The aggregate order is independent of the single-provider order. Do not infer aggregate inclusion from `utility`.
- `icsUtility` preserves the daemon calendar's current summary category. It may differ from `utility` where historical behavior requires it.
- `bindings.rust` names the existing `AlertSource` variant and registered constructor. These references are checked against Rust source; the catalog cannot define or execute Rust behavior.
- `bindings.android` names the existing legacy receiver and manifest label resource. Android aggregate and generic widget labels not tied to provider IDs are recorded in `androidLegacyWidgets`.
- `android.searchAliases` and `android.location` are Android search metadata. `aliases` is reserved for historical source IDs; do not put ordinary search terms there.
- `card` stores the current standalone card name, icon, and color for each canonical ID. Its fallback behavior for unknown IDs remains in the card code.

To add a provider, implement its scraper and constructor in code, then add one catalog entry with explicit IDs, labels, utility, default, ordering, bindings, and per-surface capabilities. Add it to aggregate widgets or the Home Assistant add-on only when intended. Run generation and review every generated diff.

## Commands

```text
npm run catalog:validate
npm run catalog:self-test
npm run catalog:generate
npm run catalog:check
```

`catalog:validate` rejects unknown/missing fields, duplicate IDs and aliases, bad utility values, missing locale strings, invalid orderings, capability mismatches, incompatible aggregate values, and stale Rust or Android bindings. `catalog:self-test` checks that representative invalid inputs fail with useful diagnostics. `catalog:generate` writes all adapters deterministically; `catalog:check` verifies checked-in outputs without changing them.

Generated files are checked in so direct Cargo, Gradle, Tauri, browser, and add-on builds do not need Node at runtime:

- `public/provider-catalog.generated.js` — UMD/CommonJS frontend registry (`AwariaProviderCatalog.providers`, `.byId`, `.getName(...)`).
- `public/provider-card-catalog.generated.js` — standalone UMD/CommonJS card registry (`AwariaProviderCardCatalog.find(id)`).
- `src-tauri/src/provider_catalog.generated.rs` — typed Rust metadata (`CatalogUtility`, `ProviderMetadata`, `PROVIDERS`).
- `src-tauri/gen/android/app/src/main/java/xyz/eremef/awaria/ProviderCatalog.generated.kt` — Android metadata (`GeneratedProviderCatalog.entries`, `.byId`, `.find`, `.aggregate`).
- `src-tauri/gen/android/app/src/main/res/values/provider_catalog.generated.xml` and `values-en/provider_catalog.generated.xml` — localized legacy widget labels.
- `awaria_outage_monitor/provider-sources.generated.yaml` — the ordered add-on source allowlist.
- The marked catalog block in `public/awaria-card.js` — embedded so the card works when copied on its own.
- The `schema.enabled_sources` list in `awaria_outage_monitor/config.yaml` — generated in place while preserving the add-on's defaults and other settings.

Do not hand-edit generated files. Edit `catalog.json`, then regenerate. If a build-only interface needs another field, agree on it with the consumer owners and extend the schema, generator, and generated artifacts together.
