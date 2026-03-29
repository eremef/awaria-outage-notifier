# Aplikacja Wygodnego Alarmowanie o Remontach i Infrastrukturalnych Awariach

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/5c13103e-8c9d-4067-b994-55e32fb21de2" />
</p>

Nowoczesna aplikacja desktopowa (Tauri) i mobilna (Android) zapewniające powiadomienia w czasie rzeczywistym o planowanych i awaryjnych przerwach w dostawie mediów. **AWARIA** agreguje dane od wielu dostawców w jeden przejrzysty interfejs.

## Pobieranie

[https://eremef.xyz/awaria](https://eremef.xyz/awaria)

## Wspierane Źródła

- **⚡ Prąd (Tauron)**: Planowane konserwacje i awaryjne wyłączenia prądu.
- **⚡ Prąd (Energa)**: Planowane wyłączenia prądu.
- **⚡ Prąd (Enea)**: Planowane konserwacje dla obszaru zachodniej Polski.
- **🔥 Ogrzewanie (Fortum)**: Planowane i bieżące wyłączenia ogrzewania oraz ciepłej wody.
- **💧 Woda (MPWiK)**: Awarie wodociągowe i prace konserwacyjne (obecnie obszar Wrocławia).

## Aplikacja Android

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/f60a0982-aefc-440e-9caa-f1c0b8fbb584" />
</p>

## Funkcje

- **Logika Multi-Source**: Agreguje alerty od różnych dostawców mediów (prąd, woda itp.).
- **Wybór Źródeł**: Możliwość dostosowania rodzajów awarii widocznych w ustawieniach.
- **Wsparcie dla wielu adresów**: Monitoruj do 20 różnych lokalizacji jednocześnie.
- **Inteligentne Dopasowanie Adresu**: Wyróżnia alerty dotyczące konkretnego adresu, informując jednocześnie o sytuacji w okolicy.
- **Design Premium**:
  - **Nowoczesny Interfejs**: System Indigo - przyjazny UI z żywymi wskaźnikami źródeł.
  - **Zwijane Kategorie**: Uporządkowany widok "Twoja Lokalizacja" oraz "Pozostałe Awarie".
  - **Responsywny Tryb Ciemny/Jasny**: Natywne wsparcie dla motywów systemowych.
- **Widżety Android**:
  - **Osobne Widżety dla Źródeł**: Oddzielne widżety dla każdego dostawcy.
  - **Zoptymalizowany Układ**: Kompaktowy rozmiar 1x1 pokazujący liczbę alertów dla wybranej ulicy.
  - **Odświeżanie Jednym Tapnięciem**: Dotknij widżetu, aby natychmiast zaktualizować dane.
  - **Współdzielona Konfiguracja**: Ustawienia synchronizują się automatycznie z głównej aplikacji.
- **Prywatność Przede Wszystkim**: Brak kont w chmurze. Twoja lokalizacja i ustawienia pozostają na urządzeniu.

## Wymagania

- Node.js (v18+)
- Rust (stable)
- Android Studio & SDK (dla systemów Android)
- Globalne CLI Tauri: `npm install -g @tauri-apps/cli`

## Instalacja

1. Zainstaluj zależności:

   ```bash
   npm install
   ```

## Rozwój (Development)

### Desktop

Uruchom aplikację desktopową w trybie deweloperskim:

```bash
npm run dev
```

### Android

Uruchom na podłączonym urządzeniu z Androidem lub emulatorze:

```bash
npm run android
```

## Budowanie

### Aplikacja Desktopowa

Zbuduj paczkę produkcyjną:

```bash
npm run build
```

### APK Android

Zbuduj APK (debug/niepodpisane):

```bash
npm run android:build
```

Plik APK zostanie zapisany w lokalizacji:
`src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk`

## Architektura

- **Frontend**: Vanilla HTML/JS/CSS w folderze `public/`. System projektowy Indigo z niestandardowymi tokenami HSL.
- **Backend (Rust)**: `src-tauri/src/lib.rs` zarządza asynchronicznym pobieraniem danych z wielu API i konwertuje je na ujednolicony format `UnifiedAlert`.
- **Widżety Android**: Natywna implementacja wykorzystująca `BaseWidgetProvider` z konkretnymi klasami dla każdego dostawcy (`TauronWidgetProvider`, `MpwikWidgetProvider`). Zawiera mechanizm `WorkManager` do okresowych aktualizacji w tle.

## Ustawienia

Ustawienia są przechowywane w pliku `settings.json` w katalogu danych aplikacji:

- **Desktop**: `%APPDATA%\xyz.eremef.awaria\` (Windows)
- **Android**: `/data/user/0/xyz.eremef.awaria/files/`

## Rozwiązywanie Problemów

- **Widżet pokazuje "?"**: Ustawienia nie zostały jeszcze skonfigurowane. Otwórz główną aplikację i ustaw swoją lokalizację.
- **Błędy EOF**: Najprawdopodobniej chwilowy błąd dostępu podczas synchronizacji ustawień. Aplikacja posiada logikę ponawiania prób.
- **Brak Alertów**: Sprawdź, czy dana kategoria mediów jest włączona w ustawieniach. **Uwaga**: Dla nowych użytkowników wszystkie źródła są domyślnie wyłączone.
