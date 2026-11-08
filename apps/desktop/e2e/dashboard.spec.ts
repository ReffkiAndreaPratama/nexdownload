import { expect, test } from "@playwright/test";

const token = process.env.NEXDOWNLOAD_E2E_TOKEN;
const apiBase =
  process.env.NEXDOWNLOAD_E2E_API ?? process.env.NEXDOWNLOAD_E2E_URL ?? "http://127.0.0.1:47821";

test.describe("web client", () => {
  test("authenticates and navigates the dashboard", async ({ page }) => {
    test.skip(!token, "Set NEXDOWNLOAD_E2E_TOKEN to run the live browser test.");

    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: "Connect to your local engine" }),
    ).toBeVisible();

    await page.getByLabel("API address").fill(apiBase);
    await page.getByLabel("API token").fill(token as string);
    await page.getByRole("button", { name: "Connect" }).click();

    await expect(page.getByRole("heading", { name: "Dashboard", exact: true })).toBeVisible();
    await expect(page.getByText("Engine throughput")).toBeVisible();

    await page.getByRole("link", { name: "Downloads", exact: true }).click();
    await expect(page).toHaveURL(/\/downloads$/);
    await expect(page.getByRole("heading", { name: "Downloads", exact: true })).toBeVisible();
  });
});
