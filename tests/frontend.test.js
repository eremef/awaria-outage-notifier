/** @vitest-environment jsdom */
import { describe, it, expect } from 'vitest';
const { filterOutages, filterAlerts, formatDate } = require('../public/script.js');

describe('Frontend Logic', () => {
    describe('filterOutages (legacy)', () => {
        const mockOutages = [
            { Message: 'Planned outage at Henryka Probusa 12, Wrocław', GAID: 100 },
            { Message: 'Awaria na Probusa 5', GAID: 101 },
            { Message: 'Maintenance on Legnicka 5, Wrocław', GAID: 102 },
            { Message: 'Prace na Jana Pawła II', GAID: 103 },
            { Message: 'Utrudnienia na Pawła', GAID: 104 },
            { Message: 'Wrocław Probusa..', GAID: 105 }
        ];

        it('finds outages matching the full street name', () => {
            const filtered = filterOutages(mockOutages, 'Henryka Probusa');
            expect(filtered.some(o => o.Message.includes('Henryka Probusa'))).toBe(true);
        });

        it('finds outages matching the short street name (last part)', () => {
            const filtered = filterOutages(mockOutages, 'Henryka Probusa');
            expect(filtered.some(o => o.Message.includes('Awaria na Probusa'))).toBe(true);
        });

        it('finds outages matching significant parts (ignoring short words)', () => {
            const filtered = filterOutages(mockOutages, 'Jana Pawła II');
            expect(filtered.some(o => o.Message.includes('Pawła'))).toBe(true);
        });

        it('does not match when text does not match (GAID matching is backend-only)', () => {
            const filtered = filterOutages(mockOutages, 'Kuźnicza');
            expect(filtered).toHaveLength(0);
        });

        it('returns empty array when no match found', () => {
            const filtered = filterOutages(mockOutages, 'Main Street');
            expect(filtered).toHaveLength(0);
        });

        it('returns empty array when street name is empty', () => {
            const filtered = filterOutages(mockOutages, '');
            expect(filtered).toHaveLength(0);
        });

        it('handles case-insensitivity and "ul." prefix', () => {
            const filtered = filterOutages(mockOutages, 'UL. PROBUSA');
            expect(filtered.some(o => o.Message.toLowerCase().includes('probusa'))).toBe(true);
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
    });
});

