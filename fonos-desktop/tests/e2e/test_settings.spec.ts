/**
 * E2E tests for settings persistence.
 * Covers: INV-09 (Settings persist — change setting, verify config file updated)
 */

import { test, expect } from "@playwright/test";
import fs from "fs";
import os from "os";
import path from "path";

const CONFIG_PATH = path.join(
  os.homedir(),
  "Library",
  "Application Support",
  "com.fonos.app",
  "config.json"
);

// ---------------------------------------------------------------------------
// INV-09: Settings persistence via UI
// ---------------------------------------------------------------------------

/**
 * INV-09: Navigate to the Settings tab, change the hotkey setting, and verify
 * the config.json file on disk reflects the new value.
 *
 * This test is FAILING until the Settings tab UI is implemented with:
 *   - A hotkey input or selector (data-testid="settings-hotkey-input")
 *   - A Save button (data-testid="settings-save-btn")
 * And the Tauri backend writes the config to:
 *   ~/Library/Application Support/com.fonos.app/config.json
 */
test("INV-09: test_settings_persist — hotkey change updates config.json", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  // Navigate to the Settings tab.
  const settingsTab = page.getByRole("tab", { name: /settings/i });
  await expect(settingsTab).toBeVisible({ timeout: 5_000 });
  await settingsTab.click();

  // Change the hotkey setting.
  const hotkeyInput = page.locator('[data-testid="settings-hotkey-input"]');
  await expect(hotkeyInput).toBeVisible({ timeout: 5_000 });
  await hotkeyInput.fill("opt+shift+d");

  // Save.
  const saveBtn = page.locator('[data-testid="settings-save-btn"]');
  await expect(saveBtn).toBeEnabled();
  await saveBtn.click();

  // Wait for the save confirmation (toast or button state change).
  const savedIndicator = page.locator('[data-testid="settings-saved"]');
  await expect(savedIndicator).toBeVisible({ timeout: 5_000 });

  // Verify the config file on disk contains the new hotkey.
  const configExists = fs.existsSync(CONFIG_PATH);
  expect(
    configExists,
    `INV-09: config file not found at ${CONFIG_PATH}`
  ).toBeTruthy();

  const config = JSON.parse(fs.readFileSync(CONFIG_PATH, "utf8"));
  expect(config.hotkey).toBe(
    "opt+shift+d",
    "INV-09: config.json should contain the updated hotkey"
  );
});

/**
 * INV-09: Settings tab should display the current values from config.json
 * when opened (read-back fidelity).
 */
test("INV-09: settings tab displays current config values", async ({ page }) => {
  // Pre-write a known config.
  const testConfig = {
    hotkey: "cmd+shift+space",
    server_port: 9880,
    dictation_mode: "clean",
    llm_endpoint: null,
    selected_voice: null,
  };

  fs.mkdirSync(path.dirname(CONFIG_PATH), { recursive: true });
  fs.writeFileSync(CONFIG_PATH, JSON.stringify(testConfig, null, 2), "utf8");

  await page.goto("/");
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  const settingsTab = page.getByRole("tab", { name: /settings/i });
  await expect(settingsTab).toBeVisible({ timeout: 5_000 });
  await settingsTab.click();

  const hotkeyInput = page.locator('[data-testid="settings-hotkey-input"]');
  await expect(hotkeyInput).toBeVisible({ timeout: 5_000 });

  // The input should reflect the value from config.json.
  await expect(hotkeyInput).toHaveValue("cmd+shift+space");
});
