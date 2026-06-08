# Dodatek Awaria Outage Monitor

Awaria to narzędzie działające w tle, służące do monitorowania przerw w dostawie mediów w Polsce. Ten dodatek w pełni integruje Awarię z Home Assistant.

## Możliwości

- **Interfejs Ingress:** Dostęp do panelu Awarii bezpośrednio z paska bocznego Home Assistant.
- **Wykrywanie MQTT (Discovery):** Automatyczne tworzenie sensorów w Home Assistant dla aktywnych awarii.
- **Automatyczne rozwiązywanie adresów:** Uproszczona konfiguracja dzięki automatycznemu wyszukiwaniu pełnych danych lokalizacyjnych w bazie TERYT na podstawie samej nazwy miejscowości i ulicy.

## Wymagania

- **Broker MQTT:** Aby zyskać sensory i możliwość tworzenia automatyzacji w Home Assistant, konieczne jest posiadanie skonfigurowanego brokera MQTT (np. dodatek Mosquitto). Awaria połączy się z nim automatycznie.

## Konfiguracja

Przejdź do zakładki **Konfiguracja** tego dodatku, aby ustawić adresy i dostawców.

### Adresy (Addresses)

Dla każdego adresu, który chcesz monitorować, dodaj wpis zawierający:

- **name**: Przyjazna nazwa (np. `Dom`, `Praca`)
- **cityName**: Dokładna nazwa miejscowości (np. `Warszawa`, `Wrocław`)
- **streetName**: Dokładna nazwa ulicy **bez przedrostka "ul."** (np. `Marszałkowska`, `Rynek`)
- **houseNo**: Dokładny numer domu/budynku (np. `12`, `45/2`). Opcjonalne, ale wysoce zalecane dla precyzyjnego dopasowywania (szczególnie dla dostawców takich jak Tauron).
- **isActive**: Ustaw na `true`, aby aktywować monitorowanie tego adresu.

*Uwaga: Awaria automatycznie uzupełni województwo, powiat, gminę oraz wewnętrzne identyfikatory, korzystając ze zintegrowanej bazy danych TERYT.*

### Aktywne źródła (Enabled Sources)

Wybierz dostawców mediów, których chcesz monitorować (np. `tauron`, `pge`, `mpwik_warszawa`).

## Użytkowanie

Po skonfigurowaniu i uruchomieniu:

1. Kliknij **Otwórz interfejs WWW** (Open Web UI), aby wyświetlić pulpit Awarii.
2. W Home Assistant przejdź do **Ustawienia -> Urządzenia i usługi -> MQTT**. Powinno tam pojawić się nowe urządzenie o nazwie `Awaria` z sensorami dla każdego z wybranych dostawców.
3. Sensory będą wskazywać liczbę aktualnie trwających lokalnych awarii.
4. Sensory zawierają szczegółowe atrybuty JSON (`alerts`) z pełnymi opisami awarii, datami rozpoczęcia/zakończenia i lokalizacjami.

## Niestandardowa karta Lovelace

Zamiast wyświetlać cały interfejs aplikacji na pasku bocznym, możesz wyświetlić awarie w postaci dedykowanej karty na swoim panelu (Dashboard):

1. Skopiuj plik `public/awaria-card.js` z repozytorium do katalogu `/config/www/` w swojej instalacji Home Assistant.
2. Przejdź do **Ustawienia -> Pulpity -> Kliknij trzy kropki w prawym górnym rogu -> Zasoby -> Dodaj zasób**.
3. Ustaw adres URL na `/local/awaria-card.js` i wybierz typ zasobu **Moduł JavaScript** (JavaScript Module).
4. Odśwież stronę w przeglądarce.
5. Na swoim pulpicie Lovelace kliknij **Dodaj kartę**, wybierz **Ręczny** (Manual) i wpisz następującą konfigurację:

   ```yaml
   type: custom:awaria-card
   ```

## Synchronizacja Kalendarza

Dodatek dynamicznie generuje kanał iCalendar (`.ics`) zawierający wszystkie aktywne oraz nadchodzące awarie.

1. Zainstaluj integrację **ICS Calendar** (dostępną przez HACS), ponieważ domyślna integracja Lokalny kalendarz (Local Calendar) w Home Assistant nie obsługuje ładowania zewnętrznych/sieciowych plików `.ics`.
2. Użyj następującego wewnętrznego adresu URL do synchronizacji:

    ```text
    http://<repo-hash>-awaria-outage-monitor:8000/api/calendar.ics
    ```

    *Uwaga: Zastąp `<repo-hash>` 8-znakowym hashem wygenerowanym przez Home Assistant dla Twojego niestandardowego repozytorium (np. `385dfded-awaria-outage-monitor`). Ten hash możesz znaleźć, wchodząc na stronę dodatku w ustawieniach Home Assistant i kopiując przedrostek z paska adresu przeglądarki (np. `.../addon/385dfded_awaria_outage_monitor/info`). Jeśli zainstalowałeś dodatek lokalnie (nie z repozytorium), użyj `local-awaria-outage-monitor`.*

    *(Jeśli subskrybujesz kalendarz spoza sieci lokalnej kontenerów Home Assistant, użyj adresu `http://<ip-twojego-ha>:8000/api/calendar.ics`)*.

## Blueprint powiadomień (Automatyzacje)

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

## Ważna informacja o ustawieniach

Ponieważ konfiguracją tego dodatku zarządza Home Assistant, menu "Ustawienia" wewnątrz interfejsu WWW Awarii jest wyłączone. Wszelkich zmian w konfiguracji (dodawanie adresów, zmiana dostawców) należy dokonywać wyłącznie przez zakładkę **Konfiguracja** w panelu dodatku HA.
