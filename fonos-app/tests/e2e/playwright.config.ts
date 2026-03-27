/**
 * Playwright configuration for Fonos app E2E tests.
 *
 * Tests run against the Tauri app's WebView via WebDriver.
 * The Tauri app must be built in dev/debug mode before running.
 *
 * Setup:
 *   1. cargo tauri build --debug
 *   2. Start Fonos API server: uv run uvicorn fonos_service.server:app --port 9880
 *      (run from /Users/ethan/Projects/design/fonos)
 *   3. npx playwright test
 *
 * Tauri WebDriver requires tauri-driver:
 *   cargo install tauri-driver
 */

import { defineConfig, devices } from "@playwright/test";
import path from "path";

export default defineConfig({
  testDir: ".",
  testMatch: "**/*.spec.ts",

  /* Each test file gets its own timeout budget. */
  timeout: 60_000,
  expect: { timeout: 10_000 },

  /* Run tests serially — the Tauri app is a single shared process. */
  workers: 1,
  fullyParallel: false,

  /* Reporter: list for CI, html for local inspection. */
  reporter: [
    ["list"],
    ["html", { outputFolder: "playwright-report", open: "never" }],
  ],

  /* Global setup/teardown: start + stop the Fonos API server. */
  globalSetup: "./global-setup.ts",
  globalTeardown: "./global-teardown.ts",

  use: {
    headless: true,

    /**
     * For Tauri WebDriver the browser is the app's WKWebView.
     * When tauri-driver is available, connect via ws://localhost:4444.
     * Until then tests target webkit to mirror the macOS WKWebView environment.
     */
    browserName: "webkit",
    baseURL: "http://localhost:1420", // Tauri dev server default

    /* Capture artefacts on failure for debugging. */
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    trace: "on-first-retry",

    /* Menu bar popover approximate dimensions. */
    viewport: { width: 380, height: 600 },
  },

  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"] },
    },
  ],

  /* Output directory for test artefacts. */
  outputDir: "test-results/",
});
