/** @vitest-environment jsdom */
import { describe, it, expect } from 'vitest';
const { filterAlerts, formatDate, matchesStreetName } = require('../public/script.js');
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
            const alert = { message: 'Awaria na ul. Henryka Probusa 12' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('matches without prefix', () => {
            const alert = { message: 'Henryka Probusa 5' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('matches short name (last part)', () => {
            const alert = { message: 'ul. Probusa 1' };
            expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('matches abbreviated prefix', () => {
             const alert = { message: 'al. Henryka Probusa' };
             expect(matchesStreetName(alert, mockAddr)).toBe(true);
        });

        it('does not match wrong street', () => {
            const alert = { message: 'ul. Legnicka 1' };
            expect(matchesStreetName(alert, mockAddr)).toBe(false);
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
            // "Pawła" should match "Pawła" but "Świdnicka" should not match "Świdnicki" if using word boundaries.
            // Wait, the current implementation uses property \p{L}.
            const customAlerts = [{ message: 'Prace na ul. Świdnickiej' }];
            const filtered = filterAlerts(customAlerts, 'Świdnicka');
            // Based on the regex in script.js: (^|[^\p{L}])Świdnicka([^\p{L}]|$)
            // It will NOT match "Świdnickiej" because "j" is a letter (\p{L}).
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
            const alert = { source: 'unknown', message: 'Utrudnienia na Probusa' };
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
            // Legnicka 1 should be in "Other alerts" (rendered in timeout, so we might need to wait or just check initial state)
            expect(container.innerHTML).not.toContain('Legnicka 1');
        });
    });
});

