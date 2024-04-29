import { vi } from "vitest";
import {
  requestVerifiablePresentation,
  type CredentialRequestData,
  type CredentialsRequest,
} from "../request-verifiable-presentation";
import { credentialPresentationMock } from "./mocks";

describe("Request Verifiable Credentials function", () => {
  const credentialSubject =
    "2vtpp-r6lcd-cbfas-qbabv-wxrv5-lsrkj-c4dtb-6ets3-srlqe-xpuzf-vqe";
  const identityProvider = "https://identity.ic0.app";
  const issuerOrigin = "https://jqajs-xiaaa-aaaad-aab5q-cai.ic0.app";
  const derivationOrigin = "https://metaissuer.vc/";
  const issuerData = {
    origin: issuerOrigin,
  };
  // Source: https://github.com/dfinity/internet-identity/blob/6df217532c7e3d4d465decbd9159ceab5262ba2d/src/vc-api/src/index.ts#L9
  const VcFlowReady = {
    jsonrpc: "2.0",
    method: "vc-flow-ready",
  };
  const vcVerifiablePresentationMessageSuccess = (id: string) => ({
    id,
    jsonrpc: "2.0",
    result: {
      verifiablePresentation: credentialPresentationMock,
    },
  });
  const vcVerifiablePresentationMessageError = (id: string) => ({
    id,
    error: "Error getting the verifiable credential",
  });
  const credentialData: CredentialRequestData = {
    credentialSpec: {
      credentialType: "MembershipCredential",
      arguments: {
        organization: "DFINITY",
      },
    },
    credentialSubject,
  };

  const credentialPresentationSuccess = credentialPresentationMock;
  const unreachableFn = () => {
    expect.unreachable("this function should not be called");
  };

  beforeEach(() => {
    window.open = vi.fn();
  });

  const startVcFlow = (): Promise<{
    request: CredentialsRequest;
    options: Record<string, string>;
  }> => {
    return new Promise((resolve) => {
      window.dispatchEvent(
        new MessageEvent("message", {
          source: {
            postMessage: (
              request: CredentialsRequest,
              options: Record<string, string>,
            ) => {
              resolve({ request, options });
            },
          } as Window,
          origin: identityProvider,
          data: {
            jsonrpc: "2.0",
            method: "vc-flow-ready",
          },
        }),
      );
    });
  };

  const mockMessageFromIdentityProvider = (data: unknown) => {
    window.dispatchEvent(
      new MessageEvent("message", {
        source: {
          postMessage: vi.fn(),
        } as unknown as Window,
        origin: identityProvider,
        data,
      }),
    );
  };

  it("opens new window and calls onSuccess with a verifiable presentation", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });

    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess).toBeCalledWith(credentialPresentationMock);
    expect(window.open).toHaveBeenCalledTimes(1);
  });

  it("send expected request to the identity provider with derivationOrigin", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      derivationOrigin,
      identityProvider,
    });

    const { request, options } = await startVcFlow();
    mockMessageFromIdentityProvider(
      vcVerifiablePresentationMessageSuccess(request.id),
    );

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(request).toEqual({
      id: expect.any(String),
      jsonrpc: "2.0",
      method: "request_credential",
      params: {
        issuer: issuerData,
        credentialSpec: credentialData.credentialSpec,
        credentialSubject,
        derivationOrigin,
      },
    });
    expect(options).toEqual({ targetOrigin: identityProvider });
  });

  it("is successful with multiple flow-ready messages", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(VcFlowReady);
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));
    expect(onSuccess).toHaveBeenCalledTimes(1);
  });

  it("waits until the expected id is received", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(
      vcVerifiablePresentationMessageSuccess("wrong-id"),
    );
    expect(onSuccess).not.toHaveBeenCalled();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));
    expect(onSuccess).toHaveBeenCalledTimes(1);
  });

  it("ignores messages before starting the flow", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });
    mockMessageFromIdentityProvider({
      id: "1",
      error: "Error getting the verifiable credential",
    });
    expect(onSuccess).not.toHaveBeenCalled();
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));
    expect(onSuccess).toHaveBeenCalledTimes(1);
  });

  it("calls onError when the credential fails", async () => {
    const onError = vi.fn();
    requestVerifiablePresentation({
      onSuccess: unreachableFn,
      onError,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageError(id));
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(
      `Error getting the verifiable credential: ${vcVerifiablePresentationMessageError("id").error}`,
    );
  });

  it("calls onError when there is no verifiable presentation", async () => {
    const onError = vi.fn();
    requestVerifiablePresentation({
      onSuccess: unreachableFn,
      onError,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    const noCredential = { id, jsonrpc: "2.0" };
    mockMessageFromIdentityProvider(noCredential);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(
      `Error getting the verifiable credential: Key 'verifiablePresentation' not found in the message data: ${JSON.stringify(noCredential)}`,
    );
  });

  it("calls onError if decoding credential fails", async () => {
    const onError = vi.fn();
    requestVerifiablePresentation({
      onSuccess: unreachableFn,
      onError,
      credentialData,
      issuerData,
      derivationOrigin: undefined,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider({
      id,
      result: {
        verifiablePresentation: "invalid",
      },
    });
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(
      "Error getting the verifiable credential: Decoding credentials failed: JWTInvalid: Invalid JWT",
    );
  });

  // TODO: Add functionality after refactor.
  it.skip("ignores messages from other origins than identity provider", () =>
    new Promise<void>((done) => done()));

  // TODO: Add functionality after refactor.
  it.skip("calls onError with timeout error if flow doesn't start in five seconds", () =>
    new Promise<void>((done) => done()));

  // TODO: Add functionality after refactor.
  it.skip("calls onError if user closes identity provider window", () =>
    new Promise<void>((done) => done()));
});
