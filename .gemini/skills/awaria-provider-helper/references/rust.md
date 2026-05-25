# Rust Backend Implementation

## 1. Registering the Provider

- **`src-tauri/src/api_logic.rs`**:
  - Add to `AlertSource` enum.
  - Add to `service_voivodeships` (return `vec![...]` of supported provinces).
  - Implement city helper `is_[cityname]`.
- **`src-tauri/src/lib.rs`**:
  - `pub mod [provider_module];`
  - Push `Box::new([provider_module]::[ProviderStruct]::new())` to `PROVIDERS` in `setup()`.

## 2. Implementing `AlertProvider`

```rust
#[async_trait]
impl AlertProvider for NewProvider {
    async fn fetch(&self) -> Result<Vec<Outage>, ProviderError> {
        // 1. Fetch data using reqwest
        // 2. Parse into Outage objects
        // 3. IMPORTANT: Set item.city and item.streets explicitly
        // 4. IMPORTANT: Set is_local = None (Matcher will handle it later)
    }
}
```

## 3. Date Parsing Rules

Polish providers use inconsistent formats. Use `utils::parse_date` or custom logic:
- Replace `.` with `-`.
- Remove `godz.` or `h`.
- Handle cases with missing years (default to current year).

## 4. Local Matching

Ensure the `fetch` logic populates the `streets` field as a `Vec<String>`. The core engine uses these to match against the user's addresses to set `is_local = Some(true)`.
