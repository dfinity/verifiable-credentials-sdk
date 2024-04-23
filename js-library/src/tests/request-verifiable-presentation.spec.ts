import { requestVerifiableCredentials } from "..";

describe("Request Verifiable Credentials function", () => {
  it("returns 2", () => {
    expect(requestVerifiableCredentials()).toBe(2);
  });
});
