# Provider catalog baseline inventory

This inventory records the checked-in behavior used to seed `catalog.json`. It is a snapshot, not a claim that every source has been live-checked.

## Reconciled identities

- The frontend registry contains 63 canonical provider IDs.
- Rust `AlertSource`, the `get_providers()` constructor list, and provider `source()` implementations each cover the same 63 IDs one-to-one.
- Android's single-provider catalog covers the same 63 IDs and records the existing legacy receiver class for each one.
- The add-on schema allowlist contains 55 IDs. The catalog stores both allowlist membership and the original order; the generated YAML does not add catalog providers that were not previously allowed.
- The Android All/Tri aggregate lists contain 5 power, 20 heating, 37 water, and 1 gas source. Each source's aggregate membership and position are recorded explicitly; those positions do not follow the single-provider selection order.

## Surface differences and preserved behavior

- Frontend labels come from the existing Polish and English translation keys. Rust keeps its current Polish display name per source. Android keeps its compact, locale-specific provider names and its separate manifest receiver labels.
- Android has 66 labeled receiver components in the manifest: 63 provider receivers plus All, Tri, and the generic single-provider component. The manifest references 65 distinct `widget_label_*` resources and one generic `widget_single_provider_label` UI resource. The catalog generates the 65 widget-label resources; the generic text remains in the app translation resources because it is also shown in a layout.
- `MpwikWidgetProvider` is the legacy Android component for the canonical `mpwik_wroclaw` source. The component name is retained explicitly.
- `psg`, `gpec`, and `pwik_kalisz` retain the existing `webviewOnly` capability used by the headless daemon filter.
- The daemon's historical calendar summary puts `sec` and `lpec` under water even though the main provider utility is heating. `icsUtility` preserves this existing output.
- The standalone Home Assistant card's current known-ID display values are captured per canonical source. Unknown-ID fallback and card rendering stay in the standalone card code.

## Evidence limits

The counts and values above were read from the checked-in frontend translations and registry, Rust enum/registry/provider modules, Android catalog/manifest/resources and widget source lists, add-on schema, and card source. Provider websites, installed widget upgrades, launcher behavior, and live add-on packaging were not part of this catalog inventory.
