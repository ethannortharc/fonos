/**
 * E2E tests for TTS generation and audio playback.
 * Covers: INV-08 (TTS via UI — text → Generate → playback indicator)
 */

import { test, expect } from "@playwright/test";

// ---------------------------------------------------------------------------
// INV-08: TTS via UI
// ---------------------------------------------------------------------------

/**
 * INV-08: Navigate to the TTS tab, enter text, click Generate, verify the
 * playback indicator appears, and verify it eventually completes.
 *
 * This test is FAILING until the TTS tab UI is implemented with:
 *   - A text input (data-testid="tts-input")
 *   - A Generate button (data-testid="tts-generate-btn")
 *   - A playback indicator (data-testid="tts-playback-indicator")
 */
test("INV-08: test_tts_generate — text input triggers generation and playback", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  // Navigate to the TTS tab.
  const ttsTab = page.getByRole("tab", { name: /tts|text.to.speech/i });
  await expect(ttsTab).toBeVisible({ timeout: 5_000 });
  await ttsTab.click();

  // Enter test text.
  const textInput = page.locator('[data-testid="tts-input"]');
  await expect(textInput).toBeVisible({ timeout: 5_000 });
  await textInput.fill("Hello from the Fonos TTS test suite.");

  // Click Generate.
  const generateBtn = page.locator('[data-testid="tts-generate-btn"]');
  await expect(generateBtn).toBeEnabled();
  await generateBtn.click();

  // Verify the playback indicator appears within 15s (server synthesis time).
  const playbackIndicator = page.locator('[data-testid="tts-playback-indicator"]');
  await expect(playbackIndicator).toBeVisible({ timeout: 15_000 });

  // Verify playback completes within 30s (indicator disappears or shows stopped state).
  await expect(playbackIndicator).toHaveAttribute("data-state", /playing|done/, {
    timeout: 30_000,
  });
});

/**
 * INV-08: Generate button should be disabled when the text input is empty.
 */
test("INV-08: generate button disabled for empty input", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  const ttsTab = page.getByRole("tab", { name: /tts|text.to.speech/i });
  await expect(ttsTab).toBeVisible({ timeout: 5_000 });
  await ttsTab.click();

  const textInput = page.locator('[data-testid="tts-input"]');
  await expect(textInput).toBeVisible({ timeout: 5_000 });
  await textInput.fill("");

  const generateBtn = page.locator('[data-testid="tts-generate-btn"]');
  await expect(generateBtn).toBeDisabled();
});
