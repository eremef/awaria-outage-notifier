import { test, expect } from '@playwright/test';
import { chromium } from 'playwright';
import AxeBuilder from '@axe-core/playwright';
import { spawn, execSync } from 'child_process';

let tauriProcess;
let browser;

test.beforeAll(async () => {
    test.setTimeout(120000); // 120s for Tauri dev to start
    console.log('Starting Tauri app for A11y tests...');
    // Start Tauri with remote debugging enabled
    tauriProcess = spawn('npx', ['tauri', 'dev'], {
        env: {
            ...process.env,
            WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9222',
        },
        shell: true,
    });

    tauriProcess.stdout.on('data', (data) => console.log(`[Tauri] ${data}`));
    tauriProcess.stderr.on('data', (data) => console.error(`[Tauri] ${data}`));

    // Wait for the DevTools port to become available
    let retries = 120; // Wait up to 120 seconds
    while (retries > 0) {
        try {
            browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
            console.log('Successfully connected to Tauri via CDP.');
            break;
        } catch (e) {
            retries--;
            await new Promise((resolve) => setTimeout(resolve, 1000));
        }
    }

    if (!browser) {
        throw new Error('Failed to connect to Tauri CDP port');
    }
});

test.afterAll(async () => {
    if (browser) await browser.close();
    if (tauriProcess) {
        console.log('Killing Tauri process...');
        if (process.platform === 'win32') {
            try {
                execSync(`taskkill /pid ${tauriProcess.pid} /f /t`);
            } catch (e) {
                console.error('Failed to kill tauriProcess:', e.message);
            }
        } else {
            tauriProcess.kill();
        }
    }
});

test('should not have any automatically detectable accessibility issues', async () => {
    test.setTimeout(60000); // Allow up to 60s for the test execution
    
    let page;
    // Wait until we find a page that isn't about:blank (the actual app webview)
    for (let i = 0; i < 60; i++) {
        const contexts = browser.contexts();
        if (contexts.length > 0) {
            const pages = contexts[0].pages();
            page = pages.find(p => p.url() !== 'about:blank' && !p.url().includes('devtools'));
            if (page) {
                // Ensure the page isn't closed before we can use it
                try {
                    await page.waitForLoadState('domcontentloaded', { timeout: 5000 });
                    await page.waitForSelector('#settings-btn', { timeout: 5000 });
                    break;
                } catch (e) {
                    page = null; // Page might have been a temporary target that closed
                }
            }
        }
        await new Promise(r => setTimeout(r, 2000));
    }

    if (!page) {
        throw new Error('Failed to find the main application page in the connected browser');
    }
    
    console.log('Waiting for the app to load...');
    // Wait for some element that indicates the app is fully rendered
    await page.waitForSelector('.app-footer', { timeout: 15000 }).catch(() => {
        console.warn('Timed out waiting for .app-footer, continuing anyway...');
    });
    
    console.log('Switching to High Contrast theme for audit...');
    await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'high-contrast'));
    // Small delay to let styles apply
    await page.waitForTimeout(500);
    
    const reportViolations = (violations, stateName) => {
        if (violations.length > 0) {
            console.error(`\nAccessibility violations found in ${stateName}:`);
            for (const violation of violations) {
                console.error(`\n[${violation.impact}] ${violation.id}: ${violation.description}`);
                console.error(`Help: ${violation.helpUrl}`);
                console.error('Nodes:');
                for (const node of violation.nodes) {
                    console.error(`  - ${node.html}`);
                }
            }
        }
    };

    console.log('Running Axe audit on Initial View...');
    let results = await new AxeBuilder({ page }).analyze();
    reportViolations(results.violations, 'Initial View');
    expect(results.violations.length).toBe(0);

    console.log('Checking for collapsed alert groups to expand...');
    let collapsedCount = 0;
    let maxTries = 30; // Prevent infinite loop if UI keeps re-rendering
    while (maxTries-- > 0) {
        // Query the first collapsed group button, wait a short bit in case of re-renders
        const btn = await page.$('.collapsible.collapsed .section-label.other');
        if (!btn) break;
        try {
            await btn.click();
            collapsedCount++;
            await page.waitForTimeout(200); // let animation finish
        } catch (e) {
            // Ignore detached errors and retry
            await page.waitForTimeout(100);
        }
    }
    
    if (collapsedCount > 0) {
        console.log(`Expanded ${collapsedCount} groups.`);
        await page.waitForTimeout(500); // animation delay
        console.log('Running Axe audit on Expanded Cards...');
        results = await new AxeBuilder({ page }).analyze();
        reportViolations(results.violations, 'Expanded Cards');
        expect(results.violations.length).toBe(0);
    } else {
        console.log('No collapsed alert groups found to expand.');
    }

    console.log('Opening Settings view...');
    await page.click('#settings-btn');
    await page.waitForTimeout(500); // animation delay

    console.log('Expanding media groups in Settings...');
    const mediaGroups = await page.$$('.settings-group-header-clickable[aria-expanded="false"]');
    if (mediaGroups.length > 0) {
        console.log(`Expanding ${mediaGroups.length} media groups...`);
        for (const btn of mediaGroups) {
            await btn.click();
        }
        await page.waitForTimeout(300); // let animation finish
    }

    console.log('Running Axe audit on Settings View...');
    // Only analyze the settings view since the background might be hidden
    results = await new AxeBuilder({ page }).analyze();
    reportViolations(results.violations, 'Settings View');
    expect(results.violations.length).toBe(0);
});
