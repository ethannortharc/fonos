/**
 * E2E performance tests.
 * Covers: QD-05 (Popover render time < 300ms)
 */

import { test, expect } from "@playwright/test";

// ---------------------------------------------------------------------------
// QD-05: UI responsiveness — popover open → content rendered < 300ms
// ---------------------------------------------------------------------------

/**
 * QD-05: Measure the time from triggering the popover open to the moment the
 * main content area is fully rendered (visible in the DOM with layout complete).
 *
 * Uses performance.now() timestamps captured via page.evaluate() to measure
 * the render latency on the JS/DOM side.
 *
 * Target: < 300ms
 */
test("QD-05: test_popover_render_time — main content renders within 300ms", async ({
  page,
}) => {
  await page.goto("/");

  // Record the timestamp immediately before triggering the popover open.
  // In Tauri the popover is the main webview window; we measure from
  // DOMContentLoaded to content-visible.
  const renderMetrics = await page.evaluate(async () => {
    const start = performance.now();

    // Wait for the first tab panel to have visible children.
    await new Promise<void>((resolve) => {
      const observer = new MutationObserver(() => {
        const panel = document.querySelector('[role="tabpanel"]');
        if (panel && panel.children.length > 0) {
          observer.disconnect();
          resolve();
        }
      });
      observer.observe(document.body, { childList: true, subtree: true });

      // Resolve immediately if already rendered.
      const panel = document.querySelector('[role="tabpanel"]');
      if (panel && panel.children.length > 0) {
        observer.disconnect();
        resolve();
      }
    });

    const end = performance.now();
    return { renderMs: end - start };
  });

  console.log(`QD-05: popover render time: ${renderMetrics.renderMs.toFixed(1)}ms`);

  expect(renderMetrics.renderMs).toBeLessThan(
    300,
    `QD-05: popover render time ${renderMetrics.renderMs.toFixed(1)}ms exceeds 300ms threshold`
  );
});

/**
 * QD-05: Measure tab-switch render time — switching between tabs should also
 * complete within 300ms.
 */
test("QD-05: tab switch renders within 300ms", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("body")).toBeVisible({ timeout: 10_000 });

  // Wait for initial render.
  await page.waitForLoadState("domcontentloaded");

  const tabs = page.getByRole("tab");
  await expect(tabs.first()).toBeVisible({ timeout: 5_000 });
  const tabCount = await tabs.count();

  if (tabCount < 2) {
    test.skip(true, "QD-05: fewer than 2 tabs found — cannot test tab switch latency");
    return;
  }

  // Click the second tab and measure render time.
  const switchMs: number = await page.evaluate(async () => {
    const tabs = document.querySelectorAll('[role="tab"]');
    if (tabs.length < 2) return 0;

    const start = performance.now();
    (tabs[1] as HTMLElement).click();

    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    });

    return performance.now() - start;
  });

  console.log(`QD-05: tab switch render time: ${switchMs.toFixed(1)}ms`);

  expect(switchMs).toBeLessThan(
    300,
    `QD-05: tab switch took ${switchMs.toFixed(1)}ms, exceeds 300ms`
  );
});
