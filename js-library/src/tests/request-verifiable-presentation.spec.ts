import { requestVerifiablePresentation } from "../request-verifiable-presentation";

describe("Request Verifiable Credentials function", () => {
  it("returns 2", () => {
    expect(requestVerifiablePresentation()).toBe(2);
  });
});
