import { expect, type BrowserContext, type Page } from "@playwright/test";

export const signInWithNewUser = async ({
  page,
  context,
}: {
  page: Page;
  context: BrowserContext;
}): Promise<number> => {
  const iiPagePromise = context.waitForEvent("page");

  await page.locator("[data-tid=login-button]").click();

  const iiPage = await iiPagePromise;
  await expect(iiPage).toHaveTitle("Internet Identity");

  await iiPage.locator("#registerButton").click();
  await iiPage.locator("[data-action=construct-identity]").click();

  await iiPage.locator("input#captchaInput").fill("a");
  await iiPage.locator("#confirmRegisterButton").click();

  try {
    const anchor = await iiPage.locator("#userNumber").textContent();
    await iiPage.locator("#displayUserContinue").click();
    await iiPage.waitForEvent("close");
    await expect(iiPage.isClosed()).toBe(true);

    if (anchor === null) {
      throw new Error("Anchor is null");
    }
    return parseInt(anchor);
  } catch (err) {
    console.error("Error:", err);
    return -1;
  }
};
