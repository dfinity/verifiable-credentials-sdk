import { Principal } from "@dfinity/principal";
import { vi } from "vitest";
import {
  ERROR_USER_INTERRUPT,
  requestVerifiablePresentation,
  type CredentialRequestData,
  type CredentialsRequest,
} from "../request-verifiable-presentation";
import { credentialPresentationMock } from "./mocks";

describe("Request Verifiable Credentials function", () => {
  const credentialSubject = Principal.fromText(
    "2vtpp-r6lcd-cbfas-qbabv-wxrv5-lsrkj-c4dtb-6ets3-srlqe-xpuzf-vqe",
  );
  const identityProvider = new URL("https://identity.ic0.app");
  const issuerOrigin = "https://jqajs-xiaaa-aaaad-aab5q-cai.ic0.app";
  const derivationOrigin = "https://metaissuer.vc/";
  const issuerData = {
    origin: issuerOrigin,
    canisterId: Principal.fromText("2222s-4iaaa-aaaaf-ax2uq-cai"),
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
    jsonrpc: "2.0",
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

  const unreachableFn = () => {
    expect.unreachable("this function should not be called");
  };

  beforeEach(() => {
    window.open = vi.fn().mockReturnValue({ close: vi.fn(), closed: false });
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    vi.useFakeTimers();
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
          origin: identityProvider.origin,
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
        origin: identityProvider.origin,
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
      identityProvider,
    });

    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess).toBeCalledWith({ Ok: credentialPresentationMock });
    expect(window.open).toHaveBeenCalledTimes(1);
  });

  it("works even with different URLs for identityProvider but same origin", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      identityProvider: new URL("https://identity.ic0.app/vc-flow"),
    });

    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess).toBeCalledWith({ Ok: credentialPresentationMock });
    expect(window.open).toHaveBeenCalledTimes(1);
  });

  it("send expected request to the identity provider with derivationOrigin and issuer canisterId", async () => {
    const onSuccess = vi.fn();
    const canisterId = Principal.fromText(
      "2vtpp-r6lcd-cbfas-qbabv-wxrv5-lsrkj-c4dtb-6ets3-srlqe-xpuzf-vqe",
    );
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData: {
        canisterId,
        origin: issuerOrigin,
      },
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
        issuer: {
          canisterId: canisterId.toText(),
          origin: issuerOrigin,
        },
        credentialSpec: credentialData.credentialSpec,
        credentialSubject: credentialSubject.toText(),
        derivationOrigin,
      },
    });
    expect(options).toEqual({ targetOrigin: identityProvider.origin });
  });

  it("is successful with multiple flow-ready messages", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(VcFlowReady);
    expect(console.warn).toHaveBeenCalledTimes(1);
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
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(
      vcVerifiablePresentationMessageSuccess("wrong-id"),
    );
    expect(console.warn).toHaveBeenCalledTimes(1);
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

  it("calls onSuccess with Error when the credential fails", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      identityProvider,
    });
    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageError(id));
    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess).toHaveBeenCalledWith({
      Err: vcVerifiablePresentationMessageError("id").error,
    });
  });

  it("calls onError when there is no verifiable presentation", async () => {
    const onError = vi.fn();
    requestVerifiablePresentation({
      onSuccess: unreachableFn,
      onError,
      credentialData,
      issuerData,
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

  it("supports multiple concurrent flows", async () => {
    const onSuccess1 = vi.fn();
    const closeWindow1 = vi.fn();
    const onSuccess2 = vi.fn();
    const closeWindow2 = vi.fn();
    window.open = vi
      .fn()
      .mockReturnValueOnce({ close: closeWindow1 })
      .mockReturnValueOnce({ close: closeWindow2 });
    requestVerifiablePresentation({
      onSuccess: onSuccess1,
      onError: unreachableFn,
      credentialData,
      issuerData,
      identityProvider,
    });
    const {
      request: { id: id1 },
    } = await startVcFlow();
    expect(onSuccess1).not.toHaveBeenCalled();
    requestVerifiablePresentation({
      onSuccess: onSuccess2,
      onError: unreachableFn,
      credentialData,
      issuerData,
      identityProvider,
    });
    const {
      request: { id: id2 },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(
      vcVerifiablePresentationMessageSuccess(id2),
    );
    expect(onSuccess2).toHaveBeenCalledTimes(1);
    expect(closeWindow2).toHaveBeenCalledTimes(1);
    expect(onSuccess1).not.toHaveBeenCalled();
    expect(closeWindow1).not.toHaveBeenCalled();
    mockMessageFromIdentityProvider(
      vcVerifiablePresentationMessageSuccess(id1),
    );
    expect(onSuccess1).toHaveBeenCalledTimes(1);
    expect(closeWindow1).toHaveBeenCalledTimes(1);
  });

  it("ignores messages from other origins than identity provider", async () => {
    const onSuccess = vi.fn();
    requestVerifiablePresentation({
      onSuccess,
      onError: unreachableFn,
      credentialData,
      issuerData,
      identityProvider,
    });

    window.dispatchEvent(
      new MessageEvent("message", {
        source: {
          postMessage: vi.fn(),
        } as unknown as Window,
        origin: "not-identity-provider",
        data: VcFlowReady,
      }),
    );
    expect(console.warn).toHaveBeenCalledTimes(1);
    const {
      request: { id },
    } = await startVcFlow();
    window.dispatchEvent(
      new MessageEvent("message", {
        source: {
          postMessage: vi.fn(),
        } as unknown as Window,
        origin: "not-identity-provider",
        data: vcVerifiablePresentationMessageSuccess(id),
      }),
    );
    expect(console.warn).toHaveBeenCalledTimes(2);
    expect(onSuccess).not.toHaveBeenCalled();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));

    expect(onSuccess).toHaveBeenCalledTimes(1);
    expect(onSuccess).toBeCalledWith({ Ok: credentialPresentationMock });
  });

  it("calls onError if user closes identity provider window", async () => {
    const onError = vi.fn();
    const DURATION_BEFORE_USER_CLOSES_WINDOW = 1000;
    window.open = vi.fn().mockImplementation(() => {
      const idpWindow = {
        closed: false,
        close: vi.fn(),
      };
      // User closes the window after 1 second
      setTimeout(() => {
        idpWindow.closed = true;
      }, DURATION_BEFORE_USER_CLOSES_WINDOW);

      return idpWindow;
    });
    requestVerifiablePresentation({
      onSuccess: unreachableFn,
      onError,
      credentialData,
      issuerData,
      identityProvider,
    });

    await startVcFlow();

    vi.advanceTimersByTime(DURATION_BEFORE_USER_CLOSES_WINDOW / 2);
    expect(onError).not.toHaveBeenCalled();

    vi.advanceTimersByTime(DURATION_BEFORE_USER_CLOSES_WINDOW / 2);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(ERROR_USER_INTERRUPT);
  });

  it("calls onError if user closes identity provider window even before the flow starts", async () => {
    const onError = vi.fn();
    const DURATION_BEFORE_USER_CLOSES_WINDOW = 1000;
    window.open = vi.fn().mockImplementation(() => {
      const idpWindow = {
        closed: false,
        close: vi.fn(),
      };
      // User closes the window after 1 second
      setTimeout(() => {
        idpWindow.closed = true;
      }, DURATION_BEFORE_USER_CLOSES_WINDOW);

      return idpWindow;
    });
    requestVerifiablePresentation({
      onSuccess: unreachableFn,
      onError,
      credentialData,
      issuerData,
      identityProvider,
    });

    vi.advanceTimersByTime(DURATION_BEFORE_USER_CLOSES_WINDOW / 2);
    expect(onError).not.toHaveBeenCalled();

    vi.advanceTimersByTime(DURATION_BEFORE_USER_CLOSES_WINDOW / 2);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(ERROR_USER_INTERRUPT);
  });

  // Identity Provider closes the window after sending the response.
  // We are checking in an interval whether the user closed the window.
  // Setting the status to "finalized" to avoid calling `onError` in `checkInterruption` while we are dealing with the response.
  // To check this in the test, I wrapped the `onSuccess` call inside a setTimeout to simulate that handling took long
  // and force the `checkValidation` to see that the window was closed.
  // Then I advanced the time in the test and checked that `onSuccess` was called, and not `onError`.
  it("should not call onError when window closes after successful flow", async () => {
    const onSuccess = vi.fn();
    const onError = vi.fn();
    const idpWindow = {
      closed: false,
      close() {
        this.closed = true;
      },
    };
    window.open = vi.fn().mockReturnValue(idpWindow);
    requestVerifiablePresentation({
      onSuccess,
      onError,
      credentialData,
      issuerData,
      identityProvider,
    });

    const {
      request: { id },
    } = await startVcFlow();
    mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess(id));

    // onSuccess is called after closing the window.
    expect(onSuccess).toHaveBeenCalledTimes(1);

    // `requestVerifiablePresentation` checks every 500ms if the window is closed.
    vi.advanceTimersByTime(600);
    expect(onError).not.toHaveBeenCalled();
  });
});
