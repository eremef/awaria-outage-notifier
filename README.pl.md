[English](README.md) | [Polski](README.pl.md)

# Aplikacja Wygodnego Alarmowania o Remontach i Infrastrukturalnych Awariach

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/3360cbd0-2de3-416f-8262-286ec796e182" />
</p>

Nowoczesna aplikacja desktopowa (Tauri) i mobilna (Android) zapewniające powiadomienia w czasie rzeczywistym o planowanych i awaryjnych przerwach w dostawie mediów. **AWARIA** agreguje dane od wielu dostawców w jeden przejrzysty interfejs.

## Pobieranie

- [Sklep Google Play](https://play.google.com/store/apps/details?id=xyz.eremef.awaria)
- [https://eremef.xyz/awaria](https://eremef.xyz/awaria)

## Wspierani Dostawcy

- **⚡ Prąd**
  - **Tauron**: Planowane konserwacje i awaryjne wyłączenia prądu.
  - **Energa**: Planowane wyłączenia prądu (Północna Polska).
  - **Enea**: Planowane konserwacje (Zachodnia Polska).
  - **PGE**: Planowane wyłączenia prądu (Wschodnia i Centralna Polska).
  - **Stoen**: Planowane wyłączenia prądu (Warszawa i okolice).
- **🔥 Gaz - PSG**: Planowane i bieżące wyłączenia gazu.
- **🌡️ Ciepło**
  - **Fortum**: Planowane i bieżące wyłączenia ogrzewania oraz ciepłej wody.
  - **Tauron Ciepło**: Przerwy w dostawie ciepła (Południowa Polska).
  - **Veolia Warszawa**: Przerwy w dostawie ciepła w Warszawie.
  - **Veolia Poznań**: Przerwy w dostawie ciepła w Poznaniu.
  - **Veolia Łódź**: Przerwy w dostawie ciepła w Łodzi.
- **💧 Woda**
  - **MPWiK Wrocław**: Awarie wodociągowe i prace konserwacyjne we Wrocławiu.
  - **MPWiK Warszawa**: Awarie i naprawy wodociągowe w Warszawie.
  - **WMK**: Awarie i konserwacje wodociągowe w Krakowie.
  - **Aquanet**: Awarie i konserwacje wodociągowe w Poznaniu i okolicach.
  - **Katowickie Wodociągi**: Awarie i konserwacje wodociągowe w Katowicach.
  - **ZWiK Łódź**: Awarie i konserwacje wodociągowe w Łodzi.
  - **PWiK Kalisz**: Awarie i konserwacje wodociągowe w Kaliszu.
  - **PWiK Częstochowa**: Awarie i konserwacje wodociągowe w Częstochowie.
  - **Wodociągi Płockie**: Awarie i konserwacje wodociągowe w Płocku.

## Aplikacja Android

## Funkcje

- **Logika Multi-Source**: Agreguje alerty od różnych dostawców mediów (prąd, woda itp.).
- **Wybór Źródeł**: Możliwość dostosowania rodzajów awarii widocznych w ustawieniach.
- **Wsparcie dla wielu adresów**: Monitoruj do 20 różnych lokalizacji jednocześnie.
- **Inteligentne Dopasowanie Adresu**: Wyróżnia alerty dotyczące konkretnego adresu (lub adresów) przy użyciu oficjalnej **bazy TERYT**, co zapewnia precyzyjne wyszukiwanie ulic i miast.
- **Powiadomienia Push w Czasie Rzeczywistym**: Otrzymuj natychmiastowe alerty na komputerze lub urządzeniu mobilnym, gdy wykryta zostanie nowa awaria dla Twojej lokalizacji.
- **Monitorowanie w Tle**: Automatyczne sprawdzanie aktualizacji w tle (interwał 30 minut na desktopie).
- **Inteligentna Pre-filtracja**: Optymalizuje użycie sieci poprzez odpytywanie tylko tych dostawców, którzy obsługują Twoje województwa.
- **Zrównoleglona Logika**: Nowoczesny backend pobierający dane od wszystkich dostawców jednocześnie z inteligentnym systemem ponawiania prób.
- **Przenośność Ustawień**: Łatwy **Eksport i Import** ustawień (adresów i włączonych źródeł), co pozwala na szybką migrację między urządzeniami lub kopię zapasową.
- **Optymalizacje Android**:
  - **Zarządzanie Baterią**: Wbudowane wsparcie dla prośby o wyłączenie optymalizacji baterii, co gwarantuje niezawodne sprawdzanie alertów i powiadomienia w tle.
  - **Natywne Widżety**: Szybki dostęp do liczby alertów bezpośrednio z ekranu głównego.
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
- **Widżety Android**: Natywna implementacja wykorzystująca `BaseWidgetProvider` z konkretnymi klasami dla każdego dostawcy (np. `TauronWidgetProvider`, `StoenWidgetProvider`). Zawiera mechanizm `WorkManager` do okresowych aktualizacji w tle.

## Ustawienia

Ustawienia są przechowywane w pliku `settings.json` w katalogu danych aplikacji:

- **Desktop**: `%APPDATA%\xyz.eremef.awaria\` (Windows)
- **Android**: `/data/user/0/xyz.eremef.awaria/files/`

## Rozwiązywanie Problemów

- **Widżet pokazuje "?"**: Ustawienia nie zostały jeszcze skonfigurowane. Otwórz główną aplikację i ustaw swoją lokalizację.
- **Błędy EOF**: Najprawdopodobniej chwilowy błąd dostępu podczas synchronizacji ustawień. Aplikacja posiada logikę ponawiania prób.
- **Brak Alertów**: Sprawdź, czy dana kategoria mediów jest włączona w ustawieniach. **Uwaga**: Dla nowych użytkowników wszystkie źródła są domyślnie wyłączone.
