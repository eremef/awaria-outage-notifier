# Awaria Constitution

## Core Principles

### I. Accurate Source Fidelity
Every `UnifiedAlert` MUST faithfully represent the official provider notice. Preserve useful
notice text, affected streets and buildings, and upstream field meaning. Keep publication time
separate from the outage interval. Values the provider does not supply MUST remain unknown rather
than being inferred without an explicit provider rule. This keeps alerts useful without inventing
facts.

### II. Shared Address and Locality Semantics
Providers MUST supply normalized city, street, and locality metadata for Awaria's shared address
matcher, using TERYT and existing matching utilities where applicable. Matching MUST tolerate
ordinary source and configured-name differences without dropping a valid alert. House-number
filtering MUST remain in `Settings.filter_by_house_no` and apply only when the user enables that
option. This keeps local/other classification consistent and respects user choice.

### III. Privacy and User Control
Saved addresses and settings MUST remain on the user's device; Awaria MUST NOT introduce a cloud
account or centralized storage for them. Network requests MUST be limited to the services needed
for the feature, and personal address data MUST NOT be sent to unrelated services. Filters that can
hide otherwise relevant alerts MUST remain under explicit user control.

### IV. Cross-Surface Consistency
The shared alert model and source metadata MUST have consistent meaning across desktop and Android.
When a provider is exposed through the frontend, widgets, daemon, or Home Assistant add-on, each
applicable integration MUST be updated or its exclusion documented. User-facing provider names,
translations, and alert details MUST remain consistent across supported surfaces.

### V. Small, Reliable Changes
Changes MUST reuse current routes, stores, models, utilities, and provider patterns where they fit.
New dependencies and shared abstractions require a concrete need. Provider failures, empty feeds,
and incomplete source data MUST be handled without fabricating alerts or preventing unrelated
providers from working. This favors dependable behavior and keeps maintenance focused.

## Technical Constraints

- Each outage provider MUST use one Rust module under `src-tauri/src/`, implement `AlertProvider`,
  and be registered in `src-tauri/src/lib.rs`.
- Provider IDs and serialized source names MUST use lowercase `snake_case`. Settings JSON MUST use
  the existing camelCase Serde convention. Preserve upstream API field casing when required and
  normalize provider data into `UnifiedAlert`.
- House-number enforcement MUST remain centralized in `Settings.filter_by_house_no`; provider
  parsers MUST NOT unconditionally reject alerts based on building numbers.
- Android signing material and other secrets MUST remain outside source control.

## Development and Quality Workflow

- Before changing application source to add a provider, create or update the root
  `implementation_plan.md` with official source evidence, coverage, data mapping, integration
  points, tests, and unresolved assumptions. The user or maintainer MUST review and approve the
  plan before provider source changes begin.
- For behavioral changes, contributors MUST identify validation appropriate to the affected
  parsing, normalization, matching, or UI behavior. Add or run tests when the user or feature
  requirements explicitly request them; otherwise follow the user's testing instruction. Report
  checks not run, and distinguish live-provider, network, and physical-device checks from
  automated results.
- Feature work MUST update relevant English and Polish documentation, translations, and platform
  resources for the surfaces it changes.
- Contributors MUST inspect their complete changes before staging or publishing. Changes MUST NOT
  be staged, committed, or pushed unless the user explicitly requests it.

## Governance

This constitution guides engineering and product decisions alongside the repository's
`AGENTS.md` instructions. Plans and code reviews MUST identify the principles affected and check
that the change follows them. Any exception requires a written rationale and maintainer approval.

Any contributor may propose an amendment. Amendments MUST update this document with their rationale
and impact, and take effect after maintainer approval. Increment the version using semantic
versioning: MAJOR for incompatible principle removals or redefinitions, MINOR for new principles or
materially expanded requirements, and PATCH for clarifications or non-semantic edits. Reviews MUST
check the version, amendment rationale, and compliance with the resulting constitution.

**Version**: 1.0.0 | **Ratified**: 2026-10-20 | **Last Amended**: 2026-10-20
