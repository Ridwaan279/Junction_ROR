import { test, expect } from "@playwright/test";
test("fixture click-through, bilingual sources, seek and reset", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (message) => {
    if (message.text().includes("Base map unavailable"))
      console.log(message.text());
  });
  await page.goto("/workspace");
  await expect(
    page.getByText("Cenário sintético", { exact: true }).first(),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Sever do Vouga (training location)" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "EN", exact: true }).click();
  await expect(
    page.getByText(
      "Training data. These observations are invented; no live incident is asserted.",
    ),
  ).toBeVisible();
  await page.getByRole("button", { name: "Sources" }).click();
  await expect(
    page.getByRole("heading", { name: "Fogos.pt", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Needs validation", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await expect(page.locator(".evidence-timeline li")).toHaveCount(1);
  const slider = page.getByRole("slider", { name: "Replay time" });
  await slider.focus();
  await slider.press("End");
  await expect(page.locator(".evidence-timeline li")).toHaveCount(2);
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator(".evidence-timeline li")).toHaveCount(1);
  await page.getByRole("button", { name: "Play", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Pause", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await page.screenshot({ path: "artifacts/desktop.png", fullPage: true });
  expect(errors).toEqual([]);
});
test("mobile stays usable with map tiles unavailable", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route("**/tile.openstreetmap.org/**", (route) => route.abort());
  await page.goto("/workspace");
  await expect(
    page.getByRole("heading", { name: "Sever do Vouga (training location)" }),
  ).toBeVisible();
  await expect(
    page.getByRole("slider", { name: "Hora da reprodução" }),
  ).toBeVisible();
  await expect(page.locator("body")).toHaveJSProperty("scrollWidth", 390);
  await page.screenshot({
    path: "artifacts/mobile-fallback.png",
    fullPage: true,
  });
});
