import { vi } from "vitest";
import {
  requestVerifiablePresentation,
  resetNextFlowId,
} from "../request-verifiable-presentation";

describe("Request Verifiable Credentials function", () => {
  const credentialSubject =
    "2vtpp-r6lcd-cbfas-qbabv-wxrv5-lsrkj-c4dtb-6ets3-srlqe-xpuzf-vqe";
  const identityProvider = "https://identity.ic0.app";
  const relyingPartyOrigin = "https://relyingparty.vc/";
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
  const verifiablePresentation = "12345";
  const expectedFlowId = "1";
  const vcVerifiablePresentationMessageSuccess = {
    // id should match the received id.
    // We know it starts with 1 because nextFlowId is incremented before the request is sent.
    // Ideally, we would record the one sent from the request and use it here.
    // TODO: Record the id passed in the reply to the first call and use it here.
    id: expectedFlowId,
    jsonrpc: "2.0",
    result: {
      verifiablePresentation,
    },
  };
  const vcVerifiablePresentationMessageError = {
    id: "1",
    error: "Error getting the verifiable credential",
  };
  const credentialData = {
    credentialSpec: {
      credentialType: "MembershipCredential",
      arguments: {
        organization: "DFINITY",
      },
    },
    credentialSubject,
  };
  let sourcePostMessageSpy;

  beforeEach(() => {
    window.open = vi.fn();
    sourcePostMessageSpy = vi.fn();
    resetNextFlowId();
  });

  const mockMessageFromIdentityProvider = (data: unknown) => {
    window.dispatchEvent(
      new MessageEvent("message", {
        source: {
          postMessage: sourcePostMessageSpy,
        } as Window,
        origin: identityProvider,
        data,
      }),
    );
  };

  it("opens new windown and calls onSuccess with a verifiable presentation", async () =>
    new Promise<void>((done) => {
      const onError = vi.fn();
      requestVerifiablePresentation({
        onSuccess: (presentation: string) => {
          expect(presentation).toEqual(verifiablePresentation);
          expect(onError).not.toHaveBeenCalled();
          done();
        },
        onError,
        credentialData,
        issuerData,
        derivationOrigin: undefined,
        identityProvider,
      });
      expect(window.open).toHaveBeenCalledTimes(1);
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess);
    }));

  it("calls onSuccess with a verifiable presentation", async () =>
    new Promise<void>((done) => {
      const onError = vi.fn();
      requestVerifiablePresentation({
        onSuccess: (presentation: string) => {
          expect(presentation).toEqual(verifiablePresentation);
          expect(onError).not.toHaveBeenCalled();
          done();
        },
        onError,
        credentialData: {
          credentialSpec: {
            credentialType: "MembershipCredential",
            arguments: {
              organization: "DFINITY",
            },
          },
          credentialSubject,
        },
        issuerData: {
          origin: issuerOrigin,
        },
        derivationOrigin: undefined,
        identityProvider,
      });
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess);
    }));

  it("send expected request to the identity provider with derivationOrigin", () =>
    new Promise<void>((done) => {
      requestVerifiablePresentation({
        onSuccess: () => {
          expect(sourcePostMessageSpy).toHaveBeenCalledTimes(1);
          expect(sourcePostMessageSpy).toHaveBeenCalledWith(
            {
              id: expectedFlowId,
              jsonrpc: "2.0",
              method: "request_credential",
              params: {
                issuer: issuerData,
                credentialSpec: credentialData.credentialSpec,
                credentialSubject,
                derivationOrigin,
              },
            },
            {
              targetOrigin: identityProvider,
            },
          );
          done();
        },
        onError: vi.fn(),
        credentialData,
        issuerData,
        derivationOrigin,
        identityProvider,
      });
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess);
    }));

  it("is successful with multiple flow-ready messages", async () =>
    new Promise<void>((done) => {
      const onError = vi.fn();
      requestVerifiablePresentation({
        onSuccess: (presentation: string) => {
          expect(presentation).toEqual(verifiablePresentation);
          expect(onError).not.toHaveBeenCalled();
          done();
        },
        onError,
        credentialData,
        issuerData,
        derivationOrigin: undefined,
        identityProvider,
      });
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess);
    }));

  it("waits until the expected id is received", () =>
    new Promise<void>((done) => {
      const onError = vi.fn();
      requestVerifiablePresentation({
        onSuccess: (presentation: string) => {
          expect(presentation).toEqual(verifiablePresentation);
          expect(onError).not.toHaveBeenCalled();
          done();
        },
        onError,
        credentialData,
        issuerData,
        derivationOrigin: undefined,
        identityProvider,
      });
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider({
        id: "not-expected-id",
        ...vcVerifiablePresentationMessageSuccess,
      });
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess);
    }));

  it("ignores messages before starting the flow", () =>
    new Promise<void>((done) => {
      const onError = vi.fn();
      requestVerifiablePresentation({
        onSuccess: (presentation: string) => {
          expect(presentation).toEqual(verifiablePresentation);
          expect(onError).not.toHaveBeenCalled();
          done();
        },
        onError,
        credentialData,
        issuerData,
        derivationOrigin: undefined,
        identityProvider,
      });
      // This doesn't trigger an error because it's before the flow starts.
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageError);
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageSuccess);
    }));

  it("calls onError when the credential fails", () =>
    new Promise<void>((done) => {
      const onSuccess = vi.fn();
      requestVerifiablePresentation({
        onSuccess,
        onError: (err: unknown) => {
          expect(onSuccess).not.toHaveBeenCalled();
          expect(err).toEqual(
            `Error getting the verifiable credential: ${vcVerifiablePresentationMessageError.error}`,
          );
          done();
        },
        credentialData,
        issuerData,
        derivationOrigin: undefined,
        identityProvider,
      });
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider(vcVerifiablePresentationMessageError);
    }));

  it("calls onError when there is no verifiable presentation", () =>
    new Promise<void>((done) => {
      const onSuccess = vi.fn();
      requestVerifiablePresentation({
        onSuccess,
        onError: (err: unknown) => {
          expect(onSuccess).not.toHaveBeenCalled();
          expect(err).toEqual("Couldn't get the verifiable credential");
          done();
        },
        credentialData,
        issuerData,
        derivationOrigin: undefined,
        identityProvider,
      });
      mockMessageFromIdentityProvider(VcFlowReady);
      mockMessageFromIdentityProvider({ id: "1", jsonrpc: "2.0" });
    }));

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
