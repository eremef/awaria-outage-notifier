# Chrome Browser Automation Learnings

## Showstoppers & Solutions

### 1. Misleading "Directory Analysis" Tool Summaries

**Problem**: When using automated browser subagents, the tool logs occasionally reported "Analyzing directory" or "Directory analysis" despite performing browser actions (clicks, typing).
**Solution**: This is a generic label used by the subagent's internal execution logic. When precision is required, use the `chrome-devtools-mcp` tools directly from the main agent context to ensure transparent and accurate logging of actions.

### 2. Handling Dynamic Autocomplete Dropdowns

**Problem**: Standard `fill` or `type_text` often fails to trigger the search results or the specific "selection" logic required by sites like Tauron (where a street must be selected from a list to populate a hidden ID).
**Solution**:

- Use `type_text` but then wait for the list to appear in the DOM.
- Identify the specific `uid` of the suggestion item (often a `button` or `listitem`) and click it explicitly.
- Verify the selection by checking the `value` attribute of the textbox or looking for new dynamic elements (like a "selected street" button).

### 3. Identifying Hidden APIs

**Problem**: The frontend UI can be complex and brittle for long-term scraping.
**Solution**: Instead of perfecting the UI automation, use `list_network_requests` immediately after a successful manual-like interaction in the browser. Look for `/iapi/` or `/api/` endpoints that return JSON. This allows for a more robust implementation using direct HTTP requests (e.g., via `reqwest` in Rust) instead of driving a browser.

### 4. Cookie Consent & CMP Widgets

**Problem**: Cookiebot and other CMP widgets can block or overlay elements, causing clicks to fail.
**Solution**: In many cases, these widgets don't actually block interaction with inputs if you target them by `uid` or pixel. If they do block, search the snapshot for "Accept" buttons or use `click_browser_pixel` to dismiss them. However, for API discovery, they can often be ignored entirely.
