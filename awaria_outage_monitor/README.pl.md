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
4. Sensory zawierają szczegółowe atrybuty JSON (`alerts`) z pełnymi opisami awarii, datami rozpoczęcia/zakończenia i lokalizacjami. Możesz użyć ich w szablonach (Templates) Home Assistant do tworzenia powiadomień!

## Ważna informacja o ustawieniach

Ponieważ konfiguracją tego dodatku zarządza Home Assistant, menu "Ustawienia" wewnątrz interfejsu WWW Awarii jest wyłączone. Wszelkich zmian w konfiguracji (dodawanie adresów, zmiana dostawców) należy dokonywać wyłącznie przez zakładkę **Konfiguracja** w panelu dodatku HA.
