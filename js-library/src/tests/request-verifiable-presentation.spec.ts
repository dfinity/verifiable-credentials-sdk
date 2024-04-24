import { vi } from "vitest";
import { requestVerifiablePresentation } from "../request-verifiable-presentation";

describe("Request Verifiable Credentials function", () => {
  const credentialSubject =
    "2vtpp-r6lcd-cbfas-qbabv-wxrv5-lsrkj-c4dtb-6ets3-srlqe-xpuzf-vqe";
  const identityProvider = "https://identity.ic0.app";
  const issuerOrigin = "https://metaissuer.vc/";
  const relyingPartyOrigin = "https://relyingparty.vc/";
  // Source: https://github.com/dfinity/internet-identity/blob/6df217532c7e3d4d465decbd9159ceab5262ba2d/src/vc-api/src/index.ts#L9
  const VcFlowReady = {
    jsonrpc: "2.0",
    method: "vc-flow-ready",
  };
  const verifiablePresentation = "12345";
  const vcVerifiablePresentationMessage = {
    // id should match the received id.
    // We know it starts with 1 because nextFlowId is incremented before the request is sent.
    // Ideally, we would record the one sent from the request and use it here.
    // TODO: Record the id passed in the reply to the first call and use it here.
    id: "1",
    jsonrpc: "2.0",
    result: {
      verifiablePresentation,
    },
  };

  beforeEach(() => {
    window.open = vi.fn();
  });

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
      expect(window.open).toHaveBeenCalledTimes(1);
      window.postMessage(VcFlowReady, "*");
      window.postMessage(vcVerifiablePresentationMessage, relyingPartyOrigin);
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
      expect(window.open).toHaveBeenCalledTimes(1);
      window.postMessage(VcFlowReady, "*");
      window.postMessage(vcVerifiablePresentationMessage, "*");
    }));
});
