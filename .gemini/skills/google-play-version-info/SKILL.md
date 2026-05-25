---
name: google-play-version-info
description: Prepare new version information for the Google Play Console in Polish and English. Use this when a release is ready and you need to generate user-facing release notes.
---

# Google Play Version Info

This skill prepares formatted release notes for the Google Play Console in Polish and English.

## Instructions

When asked to generate version info:
1.  **Identify key changes**: Summarize the most important features or fixes since the last release.
2.  **Translate**: Provide the content in both Polish (`<pl-PL>`) and English (`<en-GB>`, `<en-US>`, etc.).
3.  **Format**: Use XML tags and bullet points (`•`).
4.  **Constraint**: Keep each language block under 500 characters.

## Language Tags
- Polish: `<pl-PL>`
- English: `<en-GB>`, `<en-US>`, `<en-CA>`, `<en-AU>` (Use the same translation for all)

## Style Rules
- **One point per line**: Every point MUST be on a new line.
- **User-Centric**: Only include important information, avoid technical details.
- **No Internal Bugs**: Avoid writing about fixing bugs that occurred during development after the previous release.
- **Code Block**: Output MUST be provided in a fenced code block to preserve formatting.

## Example Output

```xml
<pl-PL>
Co nowego w tej wersji:
• Odświeżony widok ustawień z płynnymi przejściami.
• Nowoczesny wygląd paska przewijania na wszystkich ekranach.
• Inteligentne zapamiętywanie pozycji na liście awarii.
</pl-PL>
<en-GB>
What's new in this version:
• Refreshed settings view with smooth transitions.
• Modern scrollbar appearance on all screens.
• Intelligent remembering of the position in the outage list.
</en-GB>
<en-US>
What's new in this version:
• Refreshed settings view with smooth transitions.
• Modern scrollbar appearance on all screens.
• Intelligent remembering of the position in the outage list.
</en-US>
```
