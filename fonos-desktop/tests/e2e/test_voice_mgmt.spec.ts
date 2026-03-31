/**
 * E2E tests for voice management UI.
 * Covers: INV-07 (Voice CRUD — clone, count, delete)
 */

import { test, expect } from "@playwright/test";

// ---------------------------------------------------------------------------
// INV-07: Voice CRUD via UI
// ---------------------------------------------------------------------------

/**
 * INV-07: Navigate to the Voices tab, count existing voices, clone one,
 * verify count increases by 1, then delete the clone and verify count
 * returns to original.
 *
 * This test is FAILING until the Voices tab UI is implemented with:
 *   - A voice list container (testable via data-testid="voice-list")
 *   - Clone buttons per voice card (data-testid="clone-voice-btn")
 *   - Delete buttons per voice card (data-testid="delete-voice-btn")
 */
test("INV-07: test_voice_crud — clone increments count, delete decrements", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  // Navigate to Voices tab.
  const voicesTab = page.getByRole("tab", { name: /voices/i });
  await expect(voicesTab).toBeVisible({ timeout: 5_000 });
  await voicesTab.click();

  // Count initial voices.
  const voiceList = page.locator('[data-testid="voice-list"]');
  await expect(voiceList).toBeVisible({ timeout: 5_000 });

  const voiceCards = voiceList.locator('[data-testid="voice-card"]');
  const initialCount = await voiceCards.count();

  expect(initialCount).toBeGreaterThan(
    0,
    "INV-07: at least one default voice must exist"
  );

  // Clone the first voice.
  const firstCloneBtn = voiceCards
    .first()
    .locator('[data-testid="clone-voice-btn"]');
  await expect(firstCloneBtn).toBeVisible();
  await firstCloneBtn.click();

  // Wait for clone progress to complete (progress indicator disappears).
  const cloneProgress = page.locator('[data-testid="clone-progress"]');
  await cloneProgress.waitFor({ state: "detached", timeout: 30_000 });

  // Verify count increased by 1.
  await expect(voiceCards).toHaveCount(initialCount + 1, {
    timeout: 5_000,
  });

  // Delete the newly cloned voice (last in the list).
  const lastDeleteBtn = voiceCards
    .last()
    .locator('[data-testid="delete-voice-btn"]');
  await expect(lastDeleteBtn).toBeVisible();
  await lastDeleteBtn.click();

  // Confirm deletion if a dialog appears.
  const confirmBtn = page.getByRole("button", { name: /confirm|delete|yes/i });
  if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
    await confirmBtn.click();
  }

  // Verify count returned to original.
  await expect(voiceCards).toHaveCount(initialCount, { timeout: 5_000 });
});
