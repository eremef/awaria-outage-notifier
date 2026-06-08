[English](README.md) | [Polski](README.pl.md)

# Aplikacja Wygodnego Alarmowania o Remontach i Infrastrukturalnych Awariach

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/3360cbd0-2de3-416f-8262-286ec796e182" />
</p>

Nowoczesna aplikacja desktopowa (Tauri) i mobilna (Android) zapewniające powiadomienia w czasie rzeczywistym o planowanych i awaryjnych przerwach w dostawie mediów. **AWARIA** agreguje dane od wielu dostawców w jeden przejrzysty interfejs.

## Pobieranie

- [Sklep Google Play](https://play.google.com/store/apps/details?id=xyz.eremef.awaria)
- [https://eremef.xyz/awaria](https://eremef.xyz/awaria)

## Inne linki

- [Facebook](https://www.facebook.com/awaria.info)

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

## Dodatek Home Assistant (Wysoce Eksperymentalny)

Awaria to narzędzie działające w tle, służące do monitorowania przerw w dostawie mediów w Polsce. Ten dodatek w pełni integruje Awarię z Home Assistant.

Aby dodać to repozytorium do sklepu z dodatkami Home Assistant:

1. Przejdź do **Ustawienia** > **Dodatki** > **Sklep z dodatkami**.
2. Kliknij trzy kropki w prawym górnym rogu i wybierz **Repozytoria**.
3. Dodaj URL repozytorium: `https://github.com/eremef/awaria-outage-notifier`.
4. Znajdź **Awaria Outage Monitor** na liście i kliknij **Zainstaluj**.

### Możliwości

- **Interfejs Ingress:** Dostęp do panelu Awarii bezpośrednio z paska bocznego Home Assistant.
- **Wykrywanie MQTT (Discovery):** Automatyczne tworzenie sensorów w Home Assistant dla aktywnych awarii.
- **Automatyczne rozwiązywanie adresów:** Uproszczona konfiguracja dzięki automatycznemu wyszukiwaniu pełnych danych lokalizacyjnych w bazie TERYT na podstawie samej nazwy miejscowości i ulicy.
- **Dynamiczny kalendarz:** Dostęp do informacji o wyłączeniach w postaci feedu iCalendar (.ics).
- **Niestandardowa karta Lovelace:** Karta Lovelace na panel HA prezentująca awarie w przejrzysty sposób.
- **Natywne zdarzenia:** Wysyłanie zdarzenia `awaria_outage` po wykryciu nowej lokalnej awarii.

### Wymagania

- **Broker MQTT:** Aby zyskać sensory i możliwość tworzenia automatyzacji w Home Assistant, konieczne jest posiadanie skonfigurowanego brokera MQTT (np. dodatek Mosquitto). Awaria połączy się z nim automatycznie.

### Konfiguracja

Przejdź do zakładki **Konfiguracja** tego dodatku, aby ustawić adresy i dostawców.

#### Adresy (Addresses)

Dla każdego adresu, który chcesz monitorować, dodaj wpis zawierający:

- **name**: Przyjazna nazwa (np. `Dom`, `Praca`)
- **cityName**: Dokładna nazwa miejscowości (np. `Warszawa`, `Wrocław`)
- **streetName**: Dokładna nazwa ulicy **bez przedrostka "ul."** (np. `Marszałkowska`, `Rynek`)
- **houseNo**: Dokładny numer domu/budynku (np. `12`, `45/2`). Opcjonalne, ale wysoce zalecane dla precyzyjnego dopasowywania (szczególnie dla dostawców takich jak Tauron).
- **isActive**: Ustaw na `true`, aby aktywować monitorowanie tego adresu.

*Uwaga: Awaria automatycznie uzupełni województwo, powiat, gminę oraz wewnętrzne identyfikatory, korzystając ze zintegrowanej bazy danych TERYT.*

#### Aktywne źródła (Enabled Sources)

Wybierz dostawców mediów, których chcesz monitorować (np. `tauron`, `pge`, `mpwik_warszawa`).

### Użytkowanie

Po skonfigurowaniu i uruchomieniu:

1. Kliknij **Otwórz interfejs WWW** (Open Web UI), aby wyświetlić pulpit Awarii.
2. W Home Assistant przejdź do **Ustawienia -> Urządzenia i usługi -> MQTT**. Powinno tam pojawić się nowe urządzenie o nazwie `Awaria` z sensorami dla każdego z wybranych dostawców.
3. Sensory będą wskazywać liczbę aktualnie trwających lokalnych awarii.
4. Sensory zawierają szczegółowe atrybuty JSON (`alerts`) z pełnymi opisami awarii, datami rozpoczęcia/zakończenia i lokalizacjami.

### Niestandardowa karta Lovelace

Zamiast wyświetlać cały interfejs aplikacji na pasku bocznym, możesz wyświetlić awarie w postaci dedykowanej karty na swoim panelu (Dashboard):

1. Skopiuj plik `public/awaria-card.js` z repozytorium do katalogu `/config/www/` w swojej instalacji Home Assistant.
2. Przejdź do **Ustawienia -> Pulpity -> Kliknij trzy kropki w prawym górnym rogu -> Zasoby -> Dodaj zasób**.
3. Ustaw adres URL na `/local/awaria-card.js` i wybierz typ zasobu **Moduł JavaScript** (JavaScript Module).
4. Odśwież stronę w przeglądarce.
5. Na swoim pulpicie Lovelace kliknij **Dodaj kartę**, wybierz **Ręczny** (Manual) i wpisz następującą konfigurację:

   ```yaml
   type: custom:awaria-card
   ```

### Synchronizacja Kalendarza

Dodatek dynamicznie generuje kanał iCalendar (`.ics`) zawierający wszystkie aktywne oraz nadchodzące awarie.

1. Zainstaluj integrację **ICS Calendar** (dostępną przez HACS), ponieważ domyślna integracja Lokalny kalendarz (Local Calendar) w Home Assistant nie obsługuje ładowania zewnętrznych/sieciowych plików `.ics`.
2. Użyj następującego wewnętrznego adresu URL do synchronizacji:

    ```text
    http://<repo-hash>-awaria-outage-monitor:8000/api/calendar.ics
    ```

    *Uwaga: Zastąp `<repo-hash>` 8-znakowym hashem wygenerowanym przez Home Assistant dla Twojego niestandardowego repozytorium (np. `385dfded-awaria-outage-monitor`). Ten hash możesz znaleźć, wchodząc na stronę dodatku w ustawieniach Home Assistant i kopiując przedrostek z paska adresu przeglądarki (np. `.../addon/385dfded_awaria_outage_monitor/info`). Jeśli zainstalowałeś dodatek lokalnie (nie z repozytorium), użyj `local-awaria-outage-monitor`.*

    *(Jeśli subskrybujesz kalendarz spoza sieci lokalnej kontenerów Home Assistant, użyj adresu `http://<ip-twojego-ha>:8000/api/calendar.ics`)*.

### Blueprint powiadomień (Automatyzacje)

Awaria wysyła natywne zdarzenie `awaria_outage` w Home Assistant, gdy wykryje nową lokalną awarię. Możesz zaimportować poniższy Blueprint, aby wysyłać powiadomienia na swój telefon (aplikację Companion):

```yaml
blueprint:
  name: Powiadomienia o Awariach (Awaria)
  description: Wyślij powiadomienie na telefon komórkowy po wykryciu nowej lokalnej awarii.
  domain: automation
  input:
    notify_device:
      name: Urządzenie docelowe
      description: Usługa powiadomień (notify), na którą ma zostać wysłane powiadomienie.
      selector:
        action: {}

trigger:
  - trigger: event
    event_type: awaria_outage

action:
  - run_action: !input notify_device
    data:
      title: "⚠️ Wyłączenie: {{ trigger.event.data.source | upper }}"
      message: >-
        Lokalizacja: {{ trigger.event.data.location }}
        Termin: {{ trigger.event.data.startDate }} do {{ trigger.event.data.endDate }}
        
        Opis: {{ trigger.event.data.message }}
```

### Ważna informacja o ustawieniach

Ponieważ konfiguracją tego dodatku zarządza Home Assistant, menu "Ustawienia" wewnątrz interfejsu WWW Awarii jest wyłączone. Wszelkich zmian w konfiguracji (dodawanie adresów, zmiana dostawców) należy dokonywać wyłącznie przez zakładkę **Konfiguracja** w panelu dodatku HA.

## Rozwiązywanie Problemów

- **Widżet pokazuje "?"**: Ustawienia nie zostały jeszcze skonfigurowane. Otwórz główną aplikację i ustaw swoją lokalizację.
- **Błędy EOF**: Najprawdopodobniej chwilowy błąd dostępu podczas synchronizacji ustawień. Aplikacja posiada logikę ponawiania prób.
- **Brak Alertów**: Sprawdź, czy dana kategoria mediów jest włączona w ustawieniach. **Uwaga**: Dla nowych użytkowników wszystkie źródła są domyślnie wyłączone.
