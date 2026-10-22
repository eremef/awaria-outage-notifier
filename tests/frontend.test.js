/** @vitest-environment jsdom */
import { describe, it, expect } from 'vitest';
const { filterAlerts, formatDate, matchesStreetName, getSources, getRegionMatchers } = require('../public/script.js');
require('../public/awaria-card.js');
describe('Frontend Logic', () => {
    describe('filtering logic', () => {
        const mockAlerts = [
            { message: 'Planned outage at Henryka Probusa 12, Wrocław', id: 100 },
            { message: 'Awaria na Probusa 5', id: 101 },
            { message: 'Maintenance on Legnicka 5, Wrocław', id: 102 },
            { message: 'Prace na Jana Pawła II', id: 103 },
            { message: 'Utrudnienia na Pawła', id: 104 },
            { message: 'Wrocław Probusa..', id: 105 },
            { message: 'ul. Marszałkowska test', id: 106 },
            { message: 'al. Jerozolimskie test', id: 107 }
        ];

        it('finds outages matching the full street name', () => {
            const filtered = filterAlerts(mockAlerts, 'Henryka Probusa');
            expect(filtered.some(o => o.message.includes('Henryka Probusa'))).toBe(true);
        });

        it('finds outages matching the short street name (last part)', () => {
            const filtered = filterAlerts(mockAlerts, 'Henryka Probusa');
            expect(filtered.some(o => o.message.includes('Awaria na Probusa'))).toBe(true);
        });

        it('finds outages matching significant parts (ignoring short words)', () => {
            const filtered = filterAlerts(mockAlerts, 'Jana Pawła II');
            expect(filtered.some(o => o.message.includes('Pawła'))).toBe(true);
        });

        it('handles normalization of prefixes in search query', () => {
            const filtered = filterAlerts(mockAlerts, 'ul. Henryka Probusa');
            expect(filtered.some(o => o.message.includes('Henryka Probusa'))).toBe(true);
        });

        it('matches prefixes in messages', () => {
            const filtered = filterAlerts(mockAlerts, 'Marszałkowska');
            expect(filtered).toHaveLength(1);
            expect(filtered[0].message).toContain('Marszałkowska');
        });

        it('does not match when text does not match', () => {
            const filtered = filterAlerts(mockAlerts, 'Kuźnicza');
            expect(filtered).toHaveLength(0);
        });

        it('returns empty array when no match found', () => {
            const filtered = filterAlerts(mockAlerts, 'Main Street');
            expect(filtered).toHaveLength(0);
        });

        it('returns empty array when street name is empty', () => {
            const filtered = filterAlerts(mockAlerts, '');
            expect(filtered).toHaveLength(0);
        });

        it('handles case-insensitivity and "ul." prefix', () => {
            const filtered = filterAlerts(mockAlerts, 'UL. PROBUSA');
            expect(filtered.some(o => o.message.toLowerCase().includes('probusa'))).toBe(true);
        });
    });

    describe('matchesStreetName', () => {
        const mockAddr = {
            cityName: 'Wrocław',
            streetName1: 'Probusa',
            streetName2: 'Henryka'
        };

        it('matches straight forward street name', () => {
            const alert = { message: 'Awaria na ul. Henryka Probusa 12', source: 'mpwik_wroclaw' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('matches without prefix', () => {
            const alert = { message: 'Henryka Probusa 5', source: 'mpwik_wroclaw' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('matches short name (last part)', () => {
            const alert = { message: 'ul. Probusa 1', source: 'mpwik_wroclaw' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('matches abbreviated prefix', () => {
            const alert = { message: 'al. Henryka Probusa', source: 'mpwik_wroclaw' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('does not match wrong street', () => {
            const alert = { message: 'ul. Legnicka 1', source: 'mpwik_wroclaw' };
            expect(matchesStreetName(alert, mockAddr)).toBe(false);
        });

        it('matches MPWiK Rzeszów only within its service area', () => {
            const rzeszow = { cityName: 'Rzeszów', cityId: 974133, voivodeship: 'Podkarpackie', streetName1: 'Nowosądecka' };
            const lublin = { cityName: 'Lublin', cityId: 959423, voivodeship: 'Lubelskie', streetName1: 'Nowosądecka' };
            const alert = { message: 'Awaria przy ul. Nowosądeckiej', source: 'mpwik_rzeszow' };
            expect(matchesStreetName(alert, rzeszow)).toBe(true);
            expect(matchesStreetName(alert, lublin)).toBe(false);
        });

        it('matches MWiK Bydgoszcz only for Bydgoszcz addresses', () => {
            const bydgoszcz = { cityName: 'Bydgoszcz', cityId: 928363, streetName1: 'Focha' };
            const torun = { cityName: 'Toruń', cityId: 929448, streetName1: 'Focha' };
            const alert = {
                message: 'Planowane wyłączenie wody: Focha 4',
                location: 'Miejscowość: Bydgoszcz',
                source: 'mwik_bydgoszcz'
            };

            expect(matchesStreetName(alert, bydgoszcz)).toBe(true);
            expect(matchesStreetName(alert, torun)).toBe(false);
        });

        it('matches Wodociągi Kieleckie notices within its localities', () => {
            const kielce = {
                cityName: 'Kielce', cityId: 945930, voivodeship: 'Świętokrzyskie',
                commune: 'Kielce', streetName1: 'Puscha'
            };
            const cedzyna = {
                cityName: 'Cedzyna', voivodeship: 'Świętokrzyskie',
                commune: 'Górno', streetName1: 'Lubrzanka'
            };
            const sameStreetElsewhere = {
                cityName: 'Łódź', cityId: 958153, voivodeship: 'Łódzkie',
                commune: 'Łódź', streetName1: 'Puscha'
            };
            const kielceAlert = {
                message: 'Awaria wodociągowa: przerwy przy ul. Puscha 15 i 17',
                location: 'Miejscowość: Kielce',
                source: 'wodociagi_kieleckie'
            };
            const cedzynaAlert = {
                message: 'Planowana przerwa w dostawie wody: ul. Lubrzanka',
                location: 'Miejscowość: Górno, Cedzyna',
                source: 'wodociagi_kieleckie'
            };

            expect(matchesStreetName(kielceAlert, kielce)).toBe(true);
            expect(matchesStreetName(kielceAlert, sameStreetElsewhere)).toBe(false);
            expect(matchesStreetName(cedzynaAlert, cedzyna)).toBe(true);
        });

        it('matches WOBI notices only within Białystok and Wasilków', () => {
            const bialystok = {
                cityName: 'Białystok', cityId: 922410, voivodeship: 'Podlaskie',
                commune: 'Białystok', streetName1: 'Chmielna'
            };
            const wasilkowVillage = {
                cityName: 'Jurowce', voivodeship: 'Podlaskie',
                commune: 'Wasilków', streetName1: 'Sadowa'
            };
            const sameStreetElsewhere = {
                cityName: 'Łomża', voivodeship: 'Podlaskie',
                commune: 'Łomża', streetName1: 'Chmielna'
            };
            const bialystokAlert = {
                message: 'Planowane wyłączenie wody przy ulicach Chmielnej i Wąskiej 15',
                location: 'Miejscowość: Białystok',
                source: 'wodociagi_bialostockie'
            };
            const wasilkowAlert = {
                message: 'Planowana przerwa w dostawie wody przy ul. Sadowej',
                location: 'Miejscowość: Wasilków',
                source: 'wodociagi_bialostockie'
            };

            expect(matchesStreetName(bialystokAlert, bialystok)).toBe(true);
            expect(matchesStreetName(bialystokAlert, sameStreetElsewhere)).toBe(false);
            expect(matchesStreetName(wasilkowAlert, wasilkowVillage)).toBe(true);
            expect(matchesStreetName(wasilkowAlert, bialystok)).toBe(false);
        });

        it('matches MPEC Rzeszów only for Rzeszów addresses', () => {
            const rzeszow = { cityName: 'Rzeszów', cityId: 974133, streetName1: 'Architektów' };
            const lublin = { cityName: 'Lublin', cityId: 959423, streetName1: 'Architektów' };
            const alert = { message: 'Przerwa w dostawie ciepła przy ul. Architektów 3', source: 'mpec_rzeszow' };
            expect(matchesStreetName(alert, rzeszow)).toBe(true);
            expect(matchesStreetName(alert, lublin)).toBe(false);
        });

        it('matches MPEC Kielce streets only for Kielce addresses', () => {
            const kielce = { cityName: 'Kielce', cityId: 945930, streetName1: 'Wrzosowa' };
            const lublin = { cityName: 'Lublin', cityId: 959423, streetName1: 'Wrzosowa' };
            const alert = {
                message: 'Awaria ciepła: zakłócenia przy ul. Wrzosowej w Kielcach',
                source: 'mpec_kielce'
            };

            expect(matchesStreetName(alert, kielce)).toBe(true);
            expect(matchesStreetName(alert, lublin)).toBe(false);
        });

        it('matches PGE Toruń streets only for Toruń addresses', () => {
            const torun = { cityName: 'Toruń', cityId: 929448, streetName1: 'Dąbrowskiego' };
            const bydgoszcz = { cityName: 'Bydgoszcz', cityId: 928363, streetName1: 'Dąbrowskiego' };
            const alert = {
                message: 'Planowane przerwy w dostawie ciepłej wody — Wymiana armatury. Dąbrowskiego 1, 4',
                location: 'Miejscowość: Toruń',
                source: 'pge_torun_heat'
            };

            expect(matchesStreetName(alert, torun)).toBe(true);
            expect(matchesStreetName(alert, bydgoszcz)).toBe(false);
        });

        it('matches Energa Ciepło Kaliskie only for Kalisz addresses', () => {
            const kalisz = { cityName: 'Kalisz', cityId: 936579, streetName1: 'Legionów' };
            const torun = { cityName: 'Toruń', cityId: 929448, streetName1: 'Legionów' };
            const alert = {
                message: 'Awaria ogrzewania przy ul. Legionów 40A',
                location: 'Miejscowość: Kalisz',
                source: 'energa_cieplo_kaliskie'
            };

            expect(matchesStreetName(alert, kalisz)).toBe(true);
            expect(matchesStreetName(alert, torun)).toBe(false);
        });

        it('matches WM Radom streets only in the provider area and notice locality', () => {
            const radom = { cityName: 'Radom', cityId: 972750, commune: 'Radom', streetName1: 'Działkowa' };
            const cerekiew = { cityName: 'Cerekiew', commune: 'Zakrzew', streetName1: 'Złota' };
            const lublin = { cityName: 'Lublin', cityId: 959423, commune: 'Lublin', streetName1: 'Działkowa' };

            expect(matchesStreetName({
                message: 'Awaria wodociągowa na ul. Działkowej',
                location: 'Miejscowość: Radom',
                source: 'wm_radom'
            }, radom)).toBe(true);
            expect(matchesStreetName({
                message: 'Przerwa w dostawie wody na ul. Złotej',
                location: 'Miejscowość: Cerekiew',
                source: 'wm_radom'
            }, cerekiew)).toBe(true);
            expect(matchesStreetName({
                message: 'Awaria wodociągowa na ul. Działkowej',
                location: 'Miejscowość: Radom',
                source: 'wm_radom'
            }, lublin)).toBe(false);
        });

        it('matches Polish street declensions (e.g. Jędrowizna / Jędrowizny)', () => {
            const addr1 = { cityName: 'Łódź', streetName1: 'Jędrowizna', streetName2: 'ul.' };
            const alert1 = { message: 'Jędrowizny 26 - wyłączenie wodociągu', source: 'zwik_lodz' };
            expect(matchesStreetName(alert1, addr1)).toBe(true);

            const addr2 = { cityName: 'Łódź', streetName1: 'Milionowa', streetName2: 'ul.' };
            const alert2 = { message: 'Milionowa (Przędzalniana - do pos. 25/27) - wyłączenie', source: 'zwik_lodz' };
            expect(matchesStreetName(alert2, addr2)).toBe(true);
        });

        it('matches ZWiK Zielona Góra streets only within its municipality', () => {
            const zielonaGora = {
                cityName: 'Zielona Góra', cityId: 988313, voivodeship: 'Lubuskie',
                streetName1: 'Chmielna'
            };
            const lubsko = {
                cityName: 'Lubsko', cityId: 988890, voivodeship: 'Lubuskie',
                streetName1: 'Chmielna'
            };
            const alert = {
                message: 'Prace remontowe na sieci wodociągowej przy ul. Chmielnej — pełna treść komunikatu',
                location: 'Miejscowość: Zielona Góra',
                source: 'zwik_zielona_gora'
            };

            expect(matchesStreetName(alert, zielonaGora)).toBe(true);
            expect(matchesStreetName(alert, lubsko)).toBe(false);
        });
    });

    describe('PGE Toruń source integration', () => {
        it('registers the provider in the alphabetically sorted heating group', () => {
            const heatingIds = getSources()
                .filter(source => source.category === 'heating')
                .map(source => source.id);
            const provider = getSources().find(source => source.id === 'pge_torun_heat');

            expect(provider).toMatchObject({
                label: 'PGE Toruń',
                category: 'heating',
                region: 'torun',
                i18nLabel: 'source_pge_torun_heat_name',
                i18nShort: 'source_pge_torun_heat_short'
            });
            expect(heatingIds).toEqual([...heatingIds].sort());
        });

        it('uses the heat icon and style in the Home Assistant card', () => {
            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('pge_torun_heat')).toEqual({
                name: 'PGE Toruń',
                icon: '🔥',
                color: '#f44336'
            });
        });
    });

    describe('MPEC Kielce source integration', () => {
        it('registers the provider in the alphabetically sorted heating group', () => {
            const sources = getSources();
            const heatingIds = sources.filter(source => source.category === 'heating').map(source => source.id);
            const provider = sources.find(source => source.id === 'mpec_kielce');

            expect(provider).toMatchObject({
                label: 'MPEC Kielce',
                category: 'heating',
                region: 'kielce',
                i18nLabel: 'source_mpec_kielce_name',
                i18nShort: 'source_mpec_kielce_short'
            });
            expect(heatingIds).toEqual([...heatingIds].sort());
        });

        it('uses the shared heat styling in the Home Assistant card', () => {
            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('mpec_kielce')).toEqual({
                name: 'MPEC Kielce',
                icon: '🔥',
                color: '#f44336'
            });
        });
    });

    describe('getSources', () => {
        it('registers OPEC Gdynia in heating order with its service-area matcher', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'opec_gdynia');
            expect(provider).toMatchObject({
                label: 'OPEC Gdynia',
                category: 'heating',
                region: 'opec_gdynia_area',
                i18nLabel: 'source_opec_gdynia_name',
                i18nShort: 'source_opec_gdynia_short'
            });
            const heatingIds = sources.filter(source => source.category === 'heating').map(source => source.id);
            expect(heatingIds).toEqual([...heatingIds].sort());
        });

        it('registers Wodociągi Kieleckie in the water category before Płock by ID', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'wodociagi_kieleckie');
            expect(provider).toMatchObject({
                label: 'Wodociągi Kieleckie',
                category: 'water',
                region: 'wodociagi_kieleckie_area'
            });
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);
            expect(waterIds.indexOf('wodociagi_kieleckie')).toBeLessThan(waterIds.indexOf('wodociagi_plockie'));
        });

        it('registers Toruńskie Wodociągi alphabetically in the water category', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'torunskie_wodociagi');
            expect(provider).toMatchObject({
                label: 'Toruńskie Wodociągi',
                category: 'water',
                region: 'torunskie_wodociagi_area'
            });
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);
            expect(waterIds).toEqual([...waterIds].sort());
        });

        it('registers Wodociągi Białostockie in the water category before other Wodociągi IDs', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'wodociagi_bialostockie');
            expect(provider).toMatchObject({
                label: 'Wodociągi Białostockie',
                category: 'water',
                region: 'wodociagi_bialostockie_area',
                i18nLabel: 'source_wodociagi_bialostockie_name',
                i18nShort: 'source_wodociagi_bialostockie_short'
            });
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);
            expect(waterIds.indexOf('wodociagi_bialostockie')).toBeLessThan(waterIds.indexOf('wodociagi_kieleckie'));
        });

        it('registers PEWIK Gdynia alphabetically as a water provider', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'pewik_gdynia');
            expect(provider).toMatchObject({
                label: 'PEWIK Gdynia',
                category: 'water',
                region: 'pewik_gdynia_area',
                i18nLabel: 'source_pewik_gdynia_name',
                i18nShort: 'source_pewik_gdynia_short'
            });
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);
            expect(waterIds).toEqual([...waterIds].sort());

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('pewik_gdynia')).toEqual({
                name: 'PEWIK Gdynia',
                icon: '💧',
                color: '#2196f3'
            });
        });

        it('registers ZWiK Zielona Góra alphabetically with generic water styling', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'zwik_zielona_gora');
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(provider).toMatchObject({
                label: 'ZWiK Zielona Góra',
                category: 'water',
                region: 'zwik_zielona_gora_area',
                i18nLabel: 'source_zwik_zielona_gora_name',
                i18nShort: 'source_zwik_zielona_gora_short'
            });
            expect(waterIds).toEqual([...waterIds].sort());

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('zwik_zielona_gora')).toMatchObject({
                icon: '💧',
                color: '#2196f3'
            });
        });
    });

    describe('ZWiK Zielona Góra service area', () => {
        const matchesArea = getRegionMatchers().zwik_zielona_gora_area;

        it('matches the city and constituent localities in the merged municipality', () => {
            expect(matchesArea({ cityName: 'Zielona Góra', cityId: 988313, voivodeship: 'Lubuskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Drzonków', cityId: 917336, voivodeship: 'Lubuskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Ochla', voivodeship: 'Lubuskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Racula', commune: 'Zielona Góra', voivodeship: 'Lubuskie' })).toBe(true);
        });

        it('rejects same-named or nearby addresses outside the municipality', () => {
            expect(matchesArea({ cityName: 'Zielona Góra', cityId: 999001, voivodeship: 'Lubuskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Lubsko', voivodeship: 'Lubuskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Ochla', voivodeship: 'Dolnośląskie' })).toBe(false);
        });
    });

    describe('PEWIK Gdynia service area', () => {
        const matchesArea = getRegionMatchers().pewik_gdynia_area;

        it('matches the listed Pomorskie localities', () => {
            expect(matchesArea({ cityName: 'Gdynia', streetName1: 'Owsiana', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Rumia', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Reda', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Pogórze', commune: 'Gmina Kosakowo', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Bolszewo', commune: 'Gmina Wejherowo', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Gdynia', district: 'Oksywie', voivodeship: 'Pomorskie' })).toBe(true);
        });

        it('rejects other suppliers and localities outside PEWIK water coverage', () => {
            expect(matchesArea({ cityName: 'Gdynia', streetName1: 'Sarnia', voivodeship: 'Pomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Gdynia', district: 'Port Gdynia', voivodeship: 'Pomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Łężyce', commune: 'Gmina Wejherowo', voivodeship: 'Pomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Sopot', voivodeship: 'Pomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Gdynia', voivodeship: 'Mazowieckie' })).toBe(false);
        });
    });

    describe('OPEC Gdynia service area', () => {
        const matchesArea = getRegionMatchers().opec_gdynia_area;

        it('matches the five published service localities, including the Kosakowo commune', () => {
            expect(matchesArea({ cityName: 'Gdynia', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Rumia', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Reda', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Pogórze', commune: 'Kosakowo', voivodeship: 'Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Wejherowo', voivodeship: 'Pomorskie' })).toBe(true);
        });

        it('rejects addresses outside the published service area', () => {
            expect(matchesArea({ cityName: 'Sopot', voivodeship: 'Pomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Gdynia', voivodeship: 'Mazowieckie' })).toBe(false);
        });
    });

    describe('Toruńskie Wodociągi service area', () => {
        const matchesArea = getRegionMatchers().torunskie_wodociagi_area;

        it('matches Toruń and the five documented neighboring communes', () => {
            expect(matchesArea({ cityName: 'Toruń', cityId: 929448, voivodeship: 'Kujawsko-Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Lubicz Dolny', commune: 'Lubicz', voivodeship: 'Kujawsko-Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Łysomice', commune: 'Łysomice', voivodeship: 'Kujawsko-Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Obrowo', commune: 'Obrowo', voivodeship: 'Kujawsko-Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Wielka Nieszawka', commune: 'Wielka Nieszawka', voivodeship: 'Kujawsko-Pomorskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Przysiek', commune: 'Zławieś Wielka', voivodeship: 'Kujawsko-Pomorskie' })).toBe(true);
        });

        it('rejects cities outside the documented supply area', () => {
            expect(matchesArea({ cityName: 'Ciechocinek', commune: 'Ciechocinek', voivodeship: 'Kujawsko-Pomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Lubicz Dolny', commune: 'Lubicz', voivodeship: 'Pomorskie' })).toBe(false);
        });
    });

    describe('Home Assistant card provider styling', () => {
        it('uses the OPEC Gdynia name and shared heat styling', () => {
            const AwariaCard = customElements.get('awaria-card');
            const card = new AwariaCard();
            expect(card.getProviderMeta('opec_gdynia')).toEqual({
                name: 'OPEC Gdynia',
                icon: '🔥',
                color: '#f44336'
            });
        });

        it('uses the shared water styling for MWiK Bydgoszcz', () => {
            const AwariaCard = customElements.get('awaria-card');
            const card = new AwariaCard();
            const provider = card.getProviderMeta('mwik_bydgoszcz');

            expect(provider.icon).toBe('💧');
            expect(provider.color).toBe('#2196f3');

            const heatProvider = card.getProviderMeta('energa_cieplo_kaliskie');
            expect(heatProvider.name).toBe('Energa Ciepło Kaliskie');
            expect(heatProvider.icon).toBe('🔥');
        });

        it('uses the full Wodociągi Kieleckie name and shared water styling', () => {
            const AwariaCard = customElements.get('awaria-card');
            const card = new AwariaCard();
            const provider = card.getProviderMeta('wodociagi_kieleckie');

            expect(provider.name).toBe('Wodociągi Kieleckie');
            expect(provider.icon).toBe('💧');
            expect(provider.color).toBe('#2196f3');
        });

        it('uses the Toruńskie Wodociągi name and shared water styling', () => {
            const AwariaCard = customElements.get('awaria-card');
            const card = new AwariaCard();
            const provider = card.getProviderMeta('torunskie_wodociagi');

            expect(provider.name).toBe('Toruńskie Wodociągi');
            expect(provider.icon).toBe('💧');
            expect(provider.color).toBe('#2196f3');
        });

        it('uses the full Wodociągi Białostockie name and shared water styling', () => {
            const AwariaCard = customElements.get('awaria-card');
            const card = new AwariaCard();
            const provider = card.getProviderMeta('wodociagi_bialostockie');

            expect(provider.name).toBe('Wodociągi Białostockie');
            expect(provider.icon).toBe('💧');
            expect(provider.color).toBe('#2196f3');
        });
    });

    describe('filterAlerts (unified)', () => {
        const mockAlerts = [
            { source: 'water', message: 'Prace na sieci wodociągowej na ulicy Gajowicka', startDate: '2026-03-12T08:30:00', endDate: '2026-04-30T00:00:00' },
            { source: 'water', message: 'Awaria sieci wodociągowej ul. Kuźnicza 12', startDate: '2026-03-17T08:00:00', endDate: '2026-03-17T16:00:00' },
            { source: 'water', message: 'Remont na Legnicka 10', startDate: '2026-03-16T06:00:00', endDate: '2026-03-16T18:00:00' },
        ];

        it('finds water alerts matching the street name', () => {
            const filtered = filterAlerts(mockAlerts, 'Gajowicka');
            expect(filtered).toHaveLength(1);
            expect(filtered[0].message).toContain('Gajowicka');
        });

        it('finds alerts matching significant words', () => {
            const filtered = filterAlerts(mockAlerts, 'Kuźnicza');
            expect(filtered).toHaveLength(1);
            expect(filtered[0].message).toContain('Kuźnicza');
        });

        it('returns empty array when no match', () => {
            const filtered = filterAlerts(mockAlerts, 'Marszałkowska');
            expect(filtered).toHaveLength(0);
        });

        it('returns empty array for empty street name', () => {
            const filtered = filterAlerts(mockAlerts, '');
            expect(filtered).toHaveLength(0);
        });

        it('returns empty array for null alerts', () => {
            const filtered = filterAlerts(null, 'Gajowicka');
            expect(filtered).toHaveLength(0);
        });

        it('normalizes street names (removes ul., al., etc.)', () => {
            const filtered = filterAlerts(mockAlerts, 'ul. Kuźnicza');
            expect(filtered).toHaveLength(1);
            expect(filtered[0].message).toContain('Kuźnicza');
        });

        it('matches only whole words or word boundaries', () => {
            const customAlerts = [{ message: 'Prace na ul. Świdnickaextra' }];
            const filtered = filterAlerts(customAlerts, 'Świdnicka');
            expect(filtered).toHaveLength(0);
        });
    });

    describe('formatDate', () => {
        it('formats a date string correctly in pl-PL locale', () => {
            const dateStr = '2024-02-12T10:30:00';
            const formatted = formatDate(dateStr);
            expect(formatted).toMatch(/12/);
            expect(formatted).toMatch(/10:30/);
        });

        it('formats a date-only value without inventing a time', () => {
            const formatted = formatDate('2026-10-06');
            expect(formatted).toMatch(/2026/);
            expect(formatted).not.toMatch(/\d{2}:\d{2}/);
        });

        it('returns empty string for null input', () => {
            expect(formatDate(null)).toBe('');
            expect(formatDate('')).toBe('');
        });

        it('returns original string if input is not a valid date', () => {
            const customStr = 'termin zostanie podany wkrótce';
            expect(formatDate(customStr)).toBe(customStr);
        });
    });
    describe('Deduplication logic (conceptual/manual)', () => {
        it('identifies duplicates by hash', () => {
            const alerts = [
                { hash: 'abc', source: 'tauron', message: 'Alert' },
                { hash: 'abc', source: 'tauron', message: 'Alert' },
                { hash: 'def', source: 'tauron', message: 'Other' }
            ];
            const seen = new Set();
            const deduplicated = alerts.filter(a => {
                if (seen.has(a.hash)) return false;
                seen.add(a.hash);
                return true;
            });
            expect(deduplicated).toHaveLength(2);
        });
    });

    describe('Notification Logic', () => {
        it('updateNotifyStatus correctly disables/enables notification checkbox', () => {
            document.body.innerHTML = `
                <div class="settings-field-row">
                    <input type="checkbox" id="source-tauron-check" checked>
                    <div class="notify-group">
                        <input type="checkbox" id="notify-tauron-check">
                    </div>
                </div>
            `;

            const { updateNotifyStatus } = require('../public/script.js');

            // Initial state check
            updateNotifyStatus('source-tauron-check', 'notify-tauron-check');
            expect(document.getElementById('notify-tauron-check').disabled).toBe(false);

            // Uncheck source
            document.getElementById('source-tauron-check').checked = false;
            updateNotifyStatus('source-tauron-check', 'notify-tauron-check');
            expect(document.getElementById('notify-tauron-check').disabled).toBe(true);
            expect(document.querySelector('.notify-group').classList.contains('notify-disabled')).toBe(true);
        });

        it('updateUpcomingStatus handles source disabling correctly', () => {
            document.body.innerHTML = `
                <div id="upcoming-row-container">
                    <input type="checkbox" id="upcoming-notify-check">
                </div>
                <div id="upcoming-adjust-container">
                    <input type="number" id="upcoming-hours-input">
                </div>
                <input type="checkbox" id="notify-tauron-check" checked>
            `;

            const { updateUpcomingStatus } = require('../public/script.js');

            // With enabled source notification
            updateUpcomingStatus();
            expect(document.getElementById('upcoming-notify-check').disabled).toBe(false);

            // Disable all source notifications
            document.getElementById('notify-tauron-check').checked = false;
            updateUpcomingStatus();
            expect(document.getElementById('upcoming-notify-check').disabled).toBe(true);
            expect(document.getElementById('upcoming-row-container').classList.contains('notify-disabled')).toBe(true);
        });
    });

    describe('renderAlerts', () => {
        const mockSettings = {
            addresses: [{ name: 'Home', isActive: true }],
            enabledSources: ['tauron']
        };

        it('renders "no outages" message when alert list is empty', () => {
            document.body.innerHTML = '<div id="outages-container"></div>';
            const container = document.getElementById('outages-container');
            const { renderAlerts } = require('../public/script.js');

            renderAlerts([], container, mockSettings, -1);

            // Should show the "Everything looks good" dashboard
            expect(container.innerHTML).toContain('Everything looks good!');
        });

        it('filters non-enabled sources', () => {
            document.body.innerHTML = '<div id="outages-container"></div>';
            const container = document.getElementById('outages-container');
            const { renderAlerts } = require('../public/script.js');

            const alerts = [
                { source: 'tauron', message: 'Tauron Alert', hash: '1', isLocal: true, addressIndex: 0 },
                { source: 'water', message: 'Water Alert', hash: '2', isLocal: true, addressIndex: 0 }
            ];

            // Only tauron enabled in mockSettings
            renderAlerts(alerts, container, mockSettings, -1);

            expect(container.innerHTML).toContain('Tauron Alert');
            expect(container.innerHTML).not.toContain('Water Alert');
        });
    });

    describe('matchesAddress', () => {
        const { matchesAddress } = require('../public/script.js');
        const addresses = [
            { name: 'Home', isActive: true, streetName1: 'Probusa', cityName: 'Wrocław' },
            { name: 'Work', isActive: true, streetName1: '', cityName: 'Oleśnica' }
        ];

        it('respects backend matching for specific sources', () => {
            const alert = { source: 'tauron', isLocal: true, addressIndex: 0 };
            expect(matchesAddress(alert, addresses, 0)).toBe(true);

            const alert2 = { source: 'tauron', isLocal: true, addressIndex: 1 };
            expect(matchesAddress(alert2, addresses, 0)).toBe(false);
        });

        it('falls back to street name matching for other sources', () => {
            const alert = { source: 'unknown', message: 'Utrudnienia na Probusa Wrocław' };
            expect(matchesAddress(alert, addresses, 0)).toBe(true);
        });

        it('matches by city name for addresses without streets', () => {
            const alert = { source: 'unknown', message: 'Awaria w mieście Oleśnica' };
            expect(matchesAddress(alert, addresses, 1)).toBe(true);

            const alert2 = { source: 'unknown', message: 'Awaria w mieście Wrocław' };
            expect(matchesAddress(alert2, addresses, 1)).toBe(false);
        });
    });

    describe('escapeHtml', () => {
        const { escapeHtml } = require('../public/script.js');
        it('escapes special characters', () => {
            expect(escapeHtml('<script>alert("xss")</script>')).toBe('&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;');
            expect(escapeHtml('Hello & Welcome')).toBe('Hello &amp; Welcome');
        });
        it('handles null or undefined', () => {
            expect(escapeHtml(null)).toBe(null);
            expect(escapeHtml(undefined)).toBe(undefined);
        });
    });

    describe('renderAlerts complex scenarios', () => {
        const { renderAlerts, setSelectedAddressIndex } = require('../public/script.js');

        it('renders empty state when no addresses', () => {
            document.body.innerHTML = '<div id="outages-container"></div>';
            const container = document.getElementById('outages-container');
            renderAlerts([], container, { addresses: [] }, -1);
            expect(container.innerHTML).toContain('Welcome to Awaria');
        });

        it('renders disabled state when all addresses inactive', () => {
            document.body.innerHTML = '<div id="outages-container"></div>';
            const container = document.getElementById('outages-container');
            renderAlerts([], container, { addresses: [{ isActive: false }] }, -1);
            expect(container.innerHTML).toContain('Monitoring Paused');
        });

        it('renders section for local and other alerts', () => {
            document.body.innerHTML = '<div id="outages-container"></div>';
            const container = document.getElementById('outages-container');
            const settings = {
                addresses: [
                    { name: 'Home', isActive: true, streetName1: 'Probusa', cityName: 'Wrocław' },
                    { name: 'Work', isActive: true, streetName1: 'Legnicka', cityName: 'Wrocław' }
                ],
                enabledSources: ['tauron']
            };
            const alerts = [
                { source: 'tauron', message: 'Probusa 1', isLocal: true, addressIndex: 0, hash: 'h1' },
                { source: 'tauron', message: 'Legnicka 1', isLocal: true, addressIndex: 1, hash: 'h2' }
            ];

            // Filter for Home (index 0)
            renderAlerts(alerts, container, settings, 0);

            expect(container.innerHTML).toContain('Your location');
            expect(container.innerHTML).toContain('Probusa 1');
            // Legnicka 1 is in "Other alerts" (now rendered synchronously alongside local alerts)
            expect(container.innerHTML).toContain('Legnicka 1');
        });
    });
});

