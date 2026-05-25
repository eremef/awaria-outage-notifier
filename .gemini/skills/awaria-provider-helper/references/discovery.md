# API Discovery for Utility Providers

## Strategy

1.  **Analyze Network Traffic**:
    *   Navigate to the provider's outage map/list.
    *   Use `list_network_requests` to find background calls.
    *   Look for keywords: `outages`, `planned`, `awarie`, `alerts`, `map`, `incidents`.
2.  **Evaluate Source Quality**:
    *   **Tier 1: JSON/REST API** (Highest priority). E.g., Tauron, Energa.
    *   **Tier 2: GeoJSON/WFS** (Common for maps). E.g., MPWiK.
    *   **Tier 3: RSS/XML**.
    *   **Tier 4: HTML Scraping** (Last resort).
3.  **Authentication/CORS**:
    *   Check if the API requires specific headers (e.g., `Referer`, `User-Agent`).
    *   Check for CMP (Cookie Management) blocks that might need bypassing via custom `reqwest` headers.

## Common Polish Terms
- `planowane`: Planned maintenance.
- `awaryjne`: Emergency failure.
- `odczyt`: Meter reading (ignore).
- `przerwa`: Interruption.
- `ulice`: Streets.
- `miejscowość`: City/Town.
