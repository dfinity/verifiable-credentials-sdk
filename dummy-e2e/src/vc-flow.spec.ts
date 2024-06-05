import { test, expect } from "@playwright/test";
import { signInWithNewUser } from "./utils/sigin-in-user.utils";

const RP_URL = process.env.RP_URL ?? "";
const ISSUER_URL = process.env.ISSUER_URL ?? "";
const II_URL = process.env.II_URL ?? "";

test("user gets credential from dummy issuer within the dummy relying party", async ({
  page,
  context,
}) => {
  await page.goto(RP_URL);
  await expect(page).toHaveTitle("Dummy Relying Party");

  await expect(await page.getByTestId("user-principal").isVisible()).toBe(
    false
  );

  // Log in with a new user
  await page.getByTestId("ii-url-input").fill(II_URL);

  await signInWithNewUser({ page, context });

  await expect(await page.getByTestId("user-principal").isVisible()).toBe(true);

  // Fill credentials
  await page.getByTestId("issuer-url-input").fill(ISSUER_URL);

  await page.getByTestId("credential-type-input").fill("Test");

  await expect(await page.getByTestId("vc-result").textContent()).toBe("-");

  // Request credentials
  const iiPagePromise = context.waitForEvent("page");
  await page.getByTestId("request-credential-button").click();

  const iiPage = await iiPagePromise;
  await expect(iiPage).toHaveTitle("Internet Identity");
  await iiPage.locator("[data-action=allow]").click();
  await iiPage.waitForEvent("close");
  await expect(iiPage.isClosed()).toBe(true);

  await expect(await page.getByTestId("vc-result").textContent()).not.toBe("-");
});
