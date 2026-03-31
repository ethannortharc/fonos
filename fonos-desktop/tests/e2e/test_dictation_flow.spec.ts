/**
 * E2E tests for the dictation flow and API proxy roundtrip.
 * Covers: INV-06 (API proxy roundtrip — invoke transcribe, verify response in UI)
 */

import { test, expect } from "@playwright/test";

const FONOS_API = "http://127.0.0.1:9880";

// ---------------------------------------------------------------------------
// INV-06: API proxy roundtrip
// ---------------------------------------------------------------------------

/**
 * INV-06: Invoke the transcribe command via the Tauri frontend and verify
 * the transcript string appears in the UI.
 *
 * Test flow:
 *   1. Navigate to the app's dictation view.
 *   2. Trigger a transcribe invocation with a known audio fixture.
 *   3. Assert the returned transcript is displayed in the results area.
 *
 * This test is FAILING until:
 *   - The Tauri app's frontend exposes a testable transcribe flow.
 *   - The invoke("transcribe_file") command is wired in commands/dictation.rs.
 */
test("INV-06: test_api_proxy — transcribe invocation returns text in UI", async ({
  page,
}) => {
  // Navigate to the app (Tauri dev server or loaded webview).
  await page.goto("/");

  // Verify the app shell loads.
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  // Locate the Dictation tab and activate it.
  const dictationTab = page.getByRole("tab", { name: /dictation/i });
  await expect(dictationTab).toBeVisible({
    timeout: 5_000,
  });
  await dictationTab.click();

  // INV-06 [NOT IMPLEMENTED]: The transcribe_file invoke endpoint is not yet
  // wired in the frontend. Once implemented, use window.__TAURI__.invoke:
  //
  //   const transcript = await page.evaluate(async () => {
  //     return await window.__TAURI__.invoke("transcribe_file", {
  //       path: "/tmp/test_audio.wav",
  //     });
  //   });
  //   expect(typeof transcript).toBe("string");
  //   expect(transcript.length).toBeGreaterThan(0);

  // Failing assertion until the above is implemented.
  throw new Error(
    "INV-06 [NOT IMPLEMENTED]: transcribe_file invoke not yet wired in frontend. " +
      "Implement commands/dictation.rs and connect to the Tauri frontend to pass this test."
  );
});

/**
 * INV-06: Verify the Fonos API /v1/health endpoint is reachable from the
 * test runner (sanity check for server connectivity).
 */
test("INV-06: fonos api health endpoint reachable", async ({ request }) => {
  const response = await request.get(`${FONOS_API}/v1/health`);
  expect(response.ok()).toBeTruthy();
});
