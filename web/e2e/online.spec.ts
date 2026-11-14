import { test, expect } from "@playwright/test";
const api = process.env.EMBERWATCH_API_TEST_URL ?? "http://127.0.0.1:8080";
test("online dispatch, reload resync, and reset clear mission outputs", async ({
  page,
  request,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const scenariosResponse = await request.get(api + "/api/scenarios");
  expect(scenariosResponse.ok()).toBe(true);
  const scenarios = await scenariosResponse.json();
  const scenario = scenarios.find(
    (item: { mode: string }) => item.mode === "synthetic",
  );
  expect(scenario).toBeTruthy();
  const query = "?run_id=" + encodeURIComponent(scenario.run_id);
  const reset = await request.post(
    api +
      "/api/scenarios/" +
      encodeURIComponent(scenario.id) +
      "/reset" +
      query,
    { data: {} },
  );
  expect(reset.ok()).toBe(true);
  await page.goto("/");
  await page.getByRole("button", { name: "EN", exact: true }).click();
  await expect(page.locator(".connection")).toHaveText("Connected");
  await page.getByRole("button", { name: "Response", exact: true }).click();
  const mission = page.getByRole("button", {
    name: "Simulate mission",
    exact: true,
  });
  await expect(mission).toBeEnabled();
  const dispatched = page.waitForResponse(
    (response) =>
      response.url().includes("/dispatch?") &&
      response.request().method() === "POST",
  );
  await mission.click();
  expect((await dispatched).ok()).toBe(true);
  await expect(
    page.getByText("Battery reserve", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Mission duration", { exact: true }),
  ).toBeVisible();
  await expect(page.locator(".map-legend")).toContainText(
    "Illustrative spread",
  );
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await expect(page.locator(".briefing-card")).toHaveCount(1);
  await expect(
    page.locator(".briefing-card .source-link").first(),
  ).toBeVisible();
  await page.reload();
  await page.getByRole("button", { name: "EN", exact: true }).click();
  await expect(page.locator(".connection")).toHaveText("Connected");
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await expect(page.locator(".briefing-card")).toHaveCount(1);
  const resetResponse = page.waitForResponse(
    (response) =>
      response.url().includes("/reset?") &&
      response.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  expect((await resetResponse).ok()).toBe(true);
  await expect(
    page.getByText("No briefing available", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Response", exact: true }).click();
  await expect(
    page.getByText("No simulated mission planned", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("No spread result available", { exact: true }),
  ).toBeVisible();
  await expect(page.locator(".map-legend")).not.toContainText(
    "Illustrative spread",
  );
  await page.screenshot({ path: "artifacts/online.png", fullPage: true });
  expect(errors).toEqual([]);
});
