/** @vitest-environment jsdom */
import { describe, it, expect } from 'vitest';
const { filterAlerts, formatDate, matchesStreetName, matchesAddress, getSources, getRegionMatchers } = require('../public/script.js');
const { translations } = require('../public/i18n.js');
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
        it('registers AQUA S.A. in alphabetic water order with localized labels', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'aqua_bielsko');
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(provider).toMatchObject({
                label: 'AQUA S.A.',
                category: 'water',
                region: 'aqua_bielsko_area',
                i18nLabel: 'source_aqua_bielsko_name',
                i18nShort: 'source_aqua_bielsko_short'
            });
            expect(waterIds).toEqual([...waterIds].sort());
            expect(translations.en.source_aqua_bielsko_name).toBe('AQUA S.A.');
            expect(translations.en.source_aqua_bielsko_short).toBe('AQUA S.A.');
            expect(translations.pl.source_aqua_bielsko_name).toBe('AQUA S.A.');
            expect(translations.pl.source_aqua_bielsko_short).toBe('AQUA S.A.');

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('aqua_bielsko')).toEqual({
                name: 'AQUA S.A.', icon: '💧', color: '#2196f3'
            });

            card.setConfig({});
            card._shadowRoot = card.attachShadow({ mode: 'open' });
            card.hass = { states: {
                'sensor.awaria_aqua_bielsko': { attributes: { alerts: [{
                    source: 'aqua_bielsko', isLocal: true, hash: 'aqua-1',
                    startDate: '2026-10-23T12:40:00', endDate: '2026-10-23T18:30:00',
                    location: 'Międzyrzecze Górne; ul. Astrów nr 2, 4',
                    message: 'Planowane prace: Międzyrzecze Górne, ul. Astrów nr 2, 4'
                }] } }
            } };
            expect(card._shadowRoot.innerHTML).toContain('Międzyrzecze Górne; ul. Astrów nr 2, 4');
            expect(card._shadowRoot.innerHTML).toContain('Planowane prace: Międzyrzecze Górne, ul. Astrów nr 2, 4');
        });

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

        it('registers LPWiK Legnica alphabetically with generic water styling', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'lpwik_legnica');
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(provider).toMatchObject({
                label: 'LPWiK Legnica',
                category: 'water',
                region: 'lpwik_legnica_area',
                i18nLabel: 'source_lpwik_legnica_name',
                i18nShort: 'source_lpwik_legnica_short'
            });
            expect(waterIds).toEqual([...waterIds].sort());

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('lpwik_legnica')).toEqual({
                name: 'LPWiK Legnica',
                icon: '💧',
                color: '#2196f3'
            });
        });

        it('registers Bytomskie Wodociągi alphabetically with Polish and English labels', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'bytomskie_wodociagi');
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(provider).toMatchObject({
                label: 'Bytomskie Wodociągi',
                category: 'water',
                region: 'bytom',
                i18nLabel: 'source_bytomskie_wodociagi_name',
                i18nShort: 'source_bytomskie_wodociagi_short'
            });
            expect(waterIds).toEqual([...waterIds].sort());
            expect(translations.en.source_bytomskie_wodociagi_name).toBe('Bytomskie Wodociągi');
            expect(translations.en.source_bytomskie_wodociagi_short).toBe('BW Bytom');
            expect(translations.pl.source_bytomskie_wodociagi_name).toBe('Bytomskie Wodociągi');
            expect(translations.pl.source_bytomskie_wodociagi_short).toBe('BW Bytom');
        });

        it('registers Dąbrowskie Wodociągi alphabetically with localized water styling', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'dabrowskie_wodociagi');
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(provider).toMatchObject({
                label: 'Dąbrowskie Wodociągi',
                category: 'water',
                region: 'dabrowskie_wodociagi_area',
                i18nLabel: 'source_dabrowskie_wodociagi_name',
                i18nShort: 'source_dabrowskie_wodociagi_short'
            });
            expect(waterIds).toEqual([...waterIds].sort());
            expect(translations.en.source_dabrowskie_wodociagi_name).toBe('Dąbrowskie Wodociągi');
            expect(translations.en.source_dabrowskie_wodociagi_short).toBe('DW Dąbrowa');
            expect(translations.pl.source_dabrowskie_wodociagi_name).toBe('Dąbrowskie Wodociągi');
            expect(translations.pl.source_dabrowskie_wodociagi_short).toBe('DW Dąbrowa');

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('dabrowskie_wodociagi')).toEqual({
                name: 'Dąbrowskie Wodociągi', icon: '💧', color: '#2196f3'
            });
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

        it('registers PWiK Gorzów alphabetically with localized labels and generic water styling', () => {
            const sources = getSources();
            const provider = sources.find(source => source.id === 'pwik_gorzow');
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(provider).toMatchObject({
                label: 'PWiK Gorzów',
                category: 'water',
                region: 'pwik_gorzow_area',
                i18nLabel: 'source_pwik_gorzow_name',
                i18nShort: 'source_pwik_gorzow_short'
            });
            expect(waterIds).toEqual([...waterIds].sort());
            expect(translations.en.source_pwik_gorzow_name).toBe('PWiK Gorzów');
            expect(translations.en.source_pwik_gorzow_short).toBe('PWiK Gorzów');
            expect(translations.pl.source_pwik_gorzow_name).toBe('PWiK Gorzów');
            expect(translations.pl.source_pwik_gorzow_short).toBe('PWiK Gorzów');

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('pwik_gorzow')).toEqual({
                name: 'PWiK Gorzów', icon: '💧', color: '#2196f3'
            });
        });

        it('registers the Gliwice heat and water providers in their sorted categories', () => {
            const sources = getSources();
            const pec = sources.find(source => source.id === 'pec_gliwice');
            const pwik = sources.find(source => source.id === 'pwik_gliwice');
            const heatingIds = sources.filter(source => source.category === 'heating').map(source => source.id);
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);

            expect(pec).toMatchObject({
                label: 'PEC Gliwice', category: 'heating', region: 'pec_gliwice_area',
                i18nLabel: 'source_pec_gliwice_name', i18nShort: 'source_pec_gliwice_short'
            });
            expect(pwik).toMatchObject({
                label: 'PWiK Gliwice', category: 'water', region: 'pwik_gliwice_area',
                i18nLabel: 'source_pwik_gliwice_name', i18nShort: 'source_pwik_gliwice_short'
            });
            expect(heatingIds).toEqual([...heatingIds].sort());
            expect(waterIds).toEqual([...waterIds].sort());
            expect(translations.en.source_pec_gliwice_name).toBe('PEC Gliwice');
            expect(translations.pl.source_pec_gliwice_name).toBe('PEC Gliwice');
            expect(translations.en.source_pwik_gliwice_short).toBe('PWiK Gliwice');
            expect(translations.pl.source_pwik_gliwice_short).toBe('PWiK Gliwice');

            const Card = window.customElements.get('awaria-card');
            const card = new Card();
            expect(card.getProviderMeta('pec_gliwice')).toEqual({
                name: 'PEC Gliwice', icon: '🔥', color: '#f44336'
            });
            expect(card.getProviderMeta('pwik_gliwice')).toEqual({
                name: 'PWiK Gliwice', icon: '💧', color: '#2196f3'
            });
        });

        it('registers the six approved water sources in sorted order with both translations', () => {
            const sources = getSources();
            const waterIds = sources.filter(source => source.category === 'water').map(source => source.id);
            const expected = [
                'mwik_koszalin', 'pwik_ruda_slaska', 'pwik_rybnik',
                'sosnowieckie_wodociagi', 'tarnowskie_wodociagi', 'zabrzanskie_wodociagi'
            ];

            expect(waterIds).toEqual([...waterIds].sort());
            for (const id of expected) {
                const source = sources.find(item => item.id === id);
                expect(source?.category).toBe('water');
                expect(translations.en[source?.i18nLabel]).toBeTruthy();
                expect(translations.en[source?.i18nShort]).toBeTruthy();
                expect(translations.pl[source?.i18nLabel]).toBeTruthy();
                expect(translations.pl[source?.i18nShort]).toBeTruthy();
            }

            const card = new (window.customElements.get('awaria-card'))();
            expect(card.getProviderMeta('mwik_koszalin')).toMatchObject({ name: 'MWiK Koszalin', icon: '💧' });
            expect(card.getProviderMeta('pwik_ruda_slaska')).toMatchObject({ name: 'PWiK Ruda Śląska', icon: '💧' });
            expect(card.getProviderMeta('pwik_rybnik')).toMatchObject({ name: 'PWiK Rybnik', icon: '💧' });
            expect(card.getProviderMeta('sosnowieckie_wodociagi')).toMatchObject({ name: 'Sosnowieckie Wodociągi', icon: '💧' });
            expect(card.getProviderMeta('tarnowskie_wodociagi')).toMatchObject({ name: 'Tarnowskie Wodociągi', icon: '💧' });
            expect(card.getProviderMeta('zabrzanskie_wodociagi')).toMatchObject({ name: 'ZPWiK Zabrze', icon: '💧' });
        });
    });

    describe('new water provider service areas', () => {
        const regions = getRegionMatchers();

        it('limits Koszalin and Sosnowiec to their supported cities and voivodeships', () => {
            expect(regions.koszalin({ cityName: 'Koszalin', cityId: 949448, voivodeship: 'Zachodniopomorskie' })).toBe(true);
            expect(regions.koszalin({ cityName: 'Kołobrzeg', cityId: 949445, voivodeship: 'Zachodniopomorskie' })).toBe(false);
            expect(regions.sosnowiec({ cityName: 'Sosnowiec', cityId: 943428, voivodeship: 'Śląskie' })).toBe(true);
            expect(regions.sosnowiec({ cityName: 'Sosnowiec', cityId: 13592, voivodeship: 'Lubelskie' })).toBe(false);
        });

        it('matches Tarnowskie Wodociągi only to the published communes and localities', () => {
            expect(regions.tarnowskie_wodociagi_area({ cityName: 'Tarnów', cityId: 981570, voivodeship: 'Małopolskie' })).toBe(true);
            expect(regions.tarnowskie_wodociagi_area({ cityName: 'Zalasowa', cityId: 829276, commune: 'Ryglice', voivodeship: 'Małopolskie' })).toBe(true);
            expect(regions.tarnowskie_wodociagi_area({ cityName: 'Machowa', cityId: 826220, commune: 'Pilzno', voivodeship: 'Podkarpackie' })).toBe(true);
            expect(regions.tarnowskie_wodociagi_area({ cityName: 'Pilzno', cityId: 982345, commune: 'Pilzno', voivodeship: 'Podkarpackie' })).toBe(false);
            expect(regions.tarnowskie_wodociagi_area({ cityName: 'Szczepanowice', cityId: 251877, commune: 'Miechów', voivodeship: 'Małopolskie' })).toBe(false);
        });

        it('limits Zabrze and Ruda Śląska to their named cities', () => {
            expect(regions.zabrze({ cityName: 'Zabrze', cityId: 945380, voivodeship: 'Śląskie' })).toBe(true);
            expect(regions.zabrze({ cityName: 'Gliwice', cityId: 940000, voivodeship: 'Śląskie' })).toBe(false);
            expect(regions.ruda_slaska({ cityName: 'Ruda Śląska', cityId: 942630, voivodeship: 'Śląskie' })).toBe(true);
            expect(regions.ruda_slaska({ cityName: 'Ruda Śląska', cityId: 942630, voivodeship: 'Mazowieckie' })).toBe(false);
        });

        it('matches Rybnik, Jejkowice, and Gaszowice without widening to Śląskie', () => {
            expect(regions.pwik_rybnik_area({ cityName: 'Rybnik', cityId: 942765, voivodeship: 'Śląskie' })).toBe(true);
            expect(regions.pwik_rybnik_area({ cityName: 'Czernica', commune: 'Gaszowice', voivodeship: 'Śląskie' })).toBe(true);
            expect(regions.pwik_rybnik_area({ cityName: 'Katowice', cityId: 938621, voivodeship: 'Śląskie' })).toBe(false);
        });
    });

    describe('PEC Gliwice service area', () => {
        const matchesArea = getRegionMatchers().pec_gliwice_area;

        it('accepts only Gliwice with the bundled TERYT identity or a name fallback', () => {
            expect(matchesArea({ cityName: 'Gliwice', cityId: 940000, voivodeship: 'Śląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Gliwice', voivodeship: 'Śląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Gliwice', cityId: '940000', voivodeship: 'Śląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Gliwice', cityId: 938670, voivodeship: 'Śląskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Bytom', cityId: 938670, voivodeship: 'Śląskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Gliwice', cityId: 940000, voivodeship: 'Małopolskie' })).toBe(false);
        });

        it('applies the region check to fallback street matches and backend-local alerts', () => {
            const alert = { source: 'pec_gliwice', message: 'Przerwa przy ul. Testowej', location: 'Gliwice; ul. Testowa' };
            const inside = { cityName: 'Gliwice', cityId: 940000, voivodeship: 'Śląskie', streetName1: 'Testowa' };
            const outside = { cityName: 'Bytom', cityId: 938670, voivodeship: 'Śląskie', streetName1: 'Testowa' };
            expect(matchesStreetName(alert, inside)).toBe(true);
            expect(matchesStreetName(alert, outside)).toBe(false);
            expect(matchesAddress({ ...alert, isLocal: true, addressIndex: 0 }, [outside], 0)).toBe(false);
        });
    });

    describe('PWiK Gliwice service area', () => {
        const matchesArea = getRegionMatchers().pwik_gliwice_area;

        it('accepts its five municipalities through TERYT IDs or locality/commune names', () => {
            const seats = [
                ['Gliwice', 940000], ['Pyskowice', 942417], ['Rudziniec', 221238],
                ['Sośnicowice', 221936], ['Zbrosławice', 225302]
            ];
            seats.forEach(([cityName, cityId]) => {
                expect(matchesArea({ cityName, cityId, voivodeship: 'Śląskie' })).toBe(true);
            });
            expect(matchesArea({ cityName: 'Niewiesze', commune: 'Gmina Rudziniec', cityId: 221240, voivodeship: 'Śląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Pyskowice', cityId: '942417', voivodeship: 'Śląskie' })).toBe(true);
        });

        it('rejects conflicting or out-of-area localities and scopes fallback street matching to the notice locality', () => {
            expect(matchesArea({ cityName: 'Gliwice', commune: 'Gliwice', cityId: 938670, voivodeship: 'Śląskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Bytom', commune: 'Bytom', cityId: 938670, voivodeship: 'Śląskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Niewiesze', commune: 'Gmina Rudziniec', voivodeship: 'Opolskie' })).toBe(false);

            const alert = {
                source: 'pwik_gliwice',
                message: 'PLANOWANE. Przerwa w dostawie wody w Gliwicach przy ul. Testowej nr 12-16.',
                location: 'Miejscowość: Gliwice; ul. Testowa 12-16'
            };
            const gliwice = { cityName: 'Gliwice', cityId: 940000, commune: 'Gliwice', voivodeship: 'Śląskie', streetName1: 'Testowa' };
            const pyskowice = { cityName: 'Pyskowice', cityId: 942417, commune: 'Pyskowice', voivodeship: 'Śląskie', streetName1: 'Testowa' };
            expect(matchesStreetName(alert, gliwice)).toBe(true);
            expect(matchesStreetName(alert, pyskowice)).toBe(false);
            expect(matchesAddress({ ...alert, isLocal: true, addressIndex: 0 }, [pyskowice], 0)).toBe(false);
        });
    });

    describe('AQUA S.A. service area', () => {
        const matchesArea = getRegionMatchers().aqua_bielsko_area;

        it('accepts addresses in each published water-supply municipality', () => {
            const covered = [
                ['Bielsko-Biała', 'Śląskie'], ['Bestwina', 'Śląskie'],
                ['Buczkowice', 'Śląskie'], ['Chybie', 'Śląskie'],
                ['Czechowice-Dziedzice', 'Śląskie'], ['Jasienica', 'Śląskie'],
                ['Jaworze', 'Śląskie'], ['Kęty', 'Małopolskie'],
                ['Kozy', 'Śląskie'], ['Porąbka', 'Śląskie'],
                ['Szczyrk', 'Śląskie'], ['Wilamowice', 'Śląskie'],
                ['Wilkowice', 'Śląskie']
            ];
            covered.forEach(([cityName, voivodeship]) => {
                expect(matchesArea({ cityName, voivodeship })).toBe(true);
            });
        });

        it('accepts a village through its supported commune but rejects nearby and wholesale-only areas', () => {
            expect(matchesArea({ cityName: 'Międzyrzecze Górne', commune: 'Jasienica', voivodeship: 'Śląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Skoczów', commune: 'Skoczów', voivodeship: 'Śląskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Andrychów', commune: 'Andrychów', voivodeship: 'Małopolskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Bielsko-Biała', voivodeship: 'Małopolskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Kęty', voivodeship: 'Śląskie' })).toBe(false);
        });

        it('applies service-area checks to fallback matches and backend-local alerts', () => {
            const alert = { source: 'aqua_bielsko', message: 'Awaria przy ul. Darniowej', location: 'Międzyrzecze Górne; ul. Darniowa' };
            const covered = { cityName: 'Międzyrzecze Górne', commune: 'Jasienica', voivodeship: 'Śląskie', streetName1: 'Darniowa' };
            const outside = { cityName: 'Skoczów', commune: 'Skoczów', voivodeship: 'Śląskie', streetName1: 'Darniowa' };
            expect(matchesStreetName(alert, covered)).toBe(true);
            expect(matchesStreetName(alert, outside)).toBe(false);
            expect(matchesAddress({ ...alert, isLocal: true, addressIndex: 0 }, [outside], 0)).toBe(false);
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

    describe('PWiK Gorzów service area', () => {
        const matchesArea = getRegionMatchers().pwik_gorzow_area;

        it('accepts Gorzów aliases and each supported municipality by city, commune, or TERYT ID', () => {
            expect(matchesArea({ cityName: 'Gorzów Wielkopolski', cityId: 935140, voivodeship: 'Lubuskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Gorzów Wlkp.', voivodeship: 'Lubuskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Gorzów', commune: 'Gorzów Wielkopolski' })).toBe(true);

            const coveredCommunes = [
                ['Jenin', 'Bogdaniec', 178672],
                ['Lubiszyn', 'Lubiszyn', 182662],
                ['Prądocin', 'Deszczno', 179520],
                ['Wojcieszyce', 'Kłodawa', 181817],
                ['Santok', 'Santok', 186708]
            ];
            coveredCommunes.forEach(([cityName, commune, cityId]) => {
                expect(matchesArea({ cityName, commune, voivodeship: 'Lubuskie' })).toBe(true);
                expect(matchesArea({ cityName: commune, cityId, voivodeship: 'Lubuskie' })).toBe(true);
            });
            expect(matchesArea({ cityName: 'Gorzów Wielkopolski', cityId: '0935140', voivodeship: 'Lubuskie' })).toBe(true);
        });

        it('rejects addresses outside Lubuskie and contradictory locality metadata', () => {
            expect(matchesArea({ cityName: 'Kłodawa', voivodeship: 'Wielkopolskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Gorzów Wielkopolski', cityId: 935140, voivodeship: 'Zachodniopomorskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Gorzów Wielkopolski', cityId: 999999, voivodeship: 'Lubuskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Santok', commune: 'Bogdaniec', voivodeship: 'Lubuskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Skwierzyna', commune: 'Skwierzyna', voivodeship: 'Lubuskie' })).toBe(false);

            const alert = {
                message: 'Awaria wodociągowa przy ul. Słonecznej',
                location: 'Gorzów Wielkopolski; ul. Słoneczna',
                source: 'pwik_gorzow'
            };
            expect(matchesStreetName(alert, {
                cityName: 'Gorzów Wielkopolski', cityId: 935140,
                voivodeship: 'Lubuskie', streetName1: 'Słoneczna'
            })).toBe(true);
            expect(matchesStreetName(alert, {
                cityName: 'Skwierzyna', voivodeship: 'Lubuskie', streetName1: 'Słoneczna'
            })).toBe(false);
            expect(matchesAddress({ ...alert, isLocal: true, addressIndex: 0 }, [{
                cityName: 'Skwierzyna', voivodeship: 'Lubuskie', streetName1: 'Słoneczna'
            }], 0)).toBe(false);
        });
    });

    describe('LPWiK Legnica service area', () => {
        const matchesArea = getRegionMatchers().lpwik_legnica_area;

        it('matches the TERYT city ID and normalized Legnica fallback', () => {
            expect(matchesArea({ cityName: 'Legnica', cityId: 954047, voivodeship: 'Dolnośląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'LEGNICA', voivodeship: 'Dolnośląskie' })).toBe(true);
            expect(matchesArea({ cityName: 'Legnica' })).toBe(true);
        });

        it('rejects other city IDs, voivodeships, and same-street matches outside Legnica', () => {
            expect(matchesArea({ cityName: 'Legnica', cityId: 999001, voivodeship: 'Dolnośląskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Legnica', voivodeship: 'Lubuskie' })).toBe(false);
            expect(matchesArea({ cityName: 'Lubin', voivodeship: 'Dolnośląskie' })).toBe(false);

            const alert = {
                message: 'Awaria wodociągowa przy ul. Kwiatowej 12',
                source: 'lpwik_legnica'
            };
            expect(matchesStreetName(alert, {
                cityName: 'Legnica', cityId: 954047, voivodeship: 'Dolnośląskie', streetName1: 'Kwiatowa'
            })).toBe(true);
            expect(matchesStreetName(alert, {
                cityName: 'Lubin', cityId: 954123, voivodeship: 'Dolnośląskie', streetName1: 'Kwiatowa'
            })).toBe(false);
        });
    });

    describe('Bytomskie Wodociągi service area', () => {
        const matchesArea = getRegionMatchers().bytom;

        it('matches Bytom by TERYT ID or normalized city name', () => {
            expect(matchesArea({ cityName: 'Bytom', cityId: 938670 })).toBe(true);
            expect(matchesArea({ cityName: '  BYTOM  ' })).toBe(true);
            expect(matchesArea({ cityName: 'Bytom', cityId: '938670' })).toBe(true);
            expect(matchesArea({ cityName: 'Bytom', voivodeship: 'Śląskie' })).toBe(true);
        });

        it('rejects other cities, including matching streets outside Bytom', () => {
            expect(matchesArea({ cityName: 'Gliwice', cityId: 938671 })).toBe(false);
            expect(matchesArea({ cityName: 'Bytom', cityId: 938671 })).toBe(false);
            expect(matchesArea({ cityName: 'Bytom', voivodeship: 'Mazowieckie' })).toBe(false);
            expect(matchesStreetName(
                { source: 'bytomskie_wodociagi', message: 'Awaria przy ul. Dworcowej' },
                { cityName: 'Gliwice', cityId: 938671, streetName1: 'Dworcowa' }
            )).toBe(false);
        });
    });

    describe('Dąbrowskie Wodociągi service area', () => {
        const matchesArea = getRegionMatchers().dabrowskie_wodociagi_area;

        it('accepts Dąbrowa Górnicza by SIMC ID and commune identity', () => {
            expect(matchesArea({
                cityName: 'Dąbrowa Górnicza', cityId: 939473, voivodeship: 'Śląskie'
            })).toBe(true);
            expect(matchesArea({
                cityName: 'Gołonóg', cityId: 123456, commune: 'Gmina Dąbrowa Górnicza', voivodeship: 'Śląskie'
            })).toBe(true);
        });

        it('rejects other cities and conflicting locality metadata', () => {
            expect(matchesArea({
                cityName: 'Katowice', cityId: 937474, commune: 'Katowice', voivodeship: 'Śląskie'
            })).toBe(false);
            expect(matchesArea({
                cityName: 'Dąbrowa Górnicza', cityId: 937474, commune: 'Katowice', voivodeship: 'Śląskie'
            })).toBe(false);
            expect(matchesArea({
                cityName: 'Dąbrowa Górnicza', cityId: 939473, voivodeship: 'Małopolskie'
            })).toBe(false);
        });

        it('applies coverage checks to backend-local alerts', () => {
            const alert = {
                source: 'dabrowskie_wodociagi',
                message: 'Przewidywana przerwa w dostawie wody przy ul. Działki',
                location: 'ul. Działki',
                isLocal: true,
                addressIndex: 0
            };
            const covered = {
                cityName: 'Dąbrowa Górnicza', cityId: 939473,
                voivodeship: 'Śląskie', streetName1: 'Działki'
            };
            const outside = {
                cityName: 'Katowice', cityId: 937474,
                voivodeship: 'Śląskie', streetName1: 'Działki'
            };
            expect(matchesAddress(alert, [covered], 0)).toBe(true);
            expect(matchesAddress(alert, [outside], 0)).toBe(false);
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

        it('uses the explicit Bytomskie Wodociągi name and shared water styling', () => {
            const AwariaCard = customElements.get('awaria-card');
            const card = new AwariaCard();

            expect(card.getProviderMeta('bytomskie_wodociagi')).toEqual({
                name: 'Bytomskie Wodociągi',
                icon: '💧',
                color: '#2196f3'
            });
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

    describe('WPWiK Wałbrzych and RPWiK Tychy sources', () => {
        it('registers both water sources and translates their labels', () => {
            expect(getSources().find(source => source.id === 'rpwik_tychy')).toMatchObject({
                category: 'water', region: 'rpwik_tychy_area',
                i18nLabel: 'source_rpwik_tychy_name', i18nShort: 'source_rpwik_tychy_short'
            });
            expect(getSources().find(source => source.id === 'wpwik_walbrzych')).toMatchObject({
                category: 'water', region: 'wpwik_walbrzych_area',
                i18nLabel: 'source_wpwik_walbrzych_name', i18nShort: 'source_wpwik_walbrzych_short'
            });
            for (const language of ['en', 'pl']) {
                expect(translations[language].source_rpwik_tychy_name).toBe('RPWiK Tychy');
                expect(translations[language].source_wpwik_walbrzych_name).toBe('WPWiK Wałbrzych');
            }
        });

        it('limits WPWiK to its Wałbrzych-area municipalities', () => {
            const inArea = {
                name: 'Home', isActive: true, cityName: 'Dziećmorowice',
                commune: 'Walim', voivodeship: 'Dolnośląskie'
            };
            const outside = {
                name: 'Other', isActive: true, cityName: 'Świdnica',
                commune: 'Świdnica', voivodeship: 'Dolnośląskie'
            };
            const matches = getRegionMatchers().wpwik_walbrzych_area;
            expect(matches(inArea)).toBe(true);
            expect(matches(outside)).toBe(false);
            expect(matchesAddress({ source: 'wpwik_walbrzych', isLocal: true, addressIndex: 0 }, [inArea], 0)).toBe(true);
            expect(matchesAddress({ source: 'wpwik_walbrzych', isLocal: true, addressIndex: 0 }, [outside], 0)).toBe(false);
        });

        it('matches RPWiK smaller localities through their service commune', () => {
            const wyry = {
                name: 'Home', isActive: true, cityName: 'Gostyń',
                commune: 'Wyry', voivodeship: 'Śląskie'
            };
            const brzeszcze = {
                name: 'Other', isActive: true, cityName: 'Jawiszowice',
                commune: 'Brzeszcze', voivodeship: 'Małopolskie'
            };
            const outside = {
                name: 'Outside', isActive: true, cityName: 'Mikołów',
                commune: 'Mikołów', voivodeship: 'Śląskie'
            };
            const matches = getRegionMatchers().rpwik_tychy_area;
            expect(matches(wyry)).toBe(true);
            expect(matches(brzeszcze)).toBe(true);
            expect(matches(outside)).toBe(false);
            expect(matchesAddress({ source: 'rpwik_tychy', isLocal: true, addressIndex: 0 }, [outside], 0)).toBe(false);
        });
    });
});

