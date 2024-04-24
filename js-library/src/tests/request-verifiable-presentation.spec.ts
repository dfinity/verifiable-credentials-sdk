import { vi } from "vitest";
import { requestVerifiablePresentation } from "../request-verifiable-presentation";

describe("Request Verifiable Credentials function", () => {
  const credentialSubject = "2vtpp-r6lcd-cbfas-qbabv-wxrv5-lsrkj-c4dtb-6ets3-srlqe-xpuzf-vqe";
  const identityProvider = "https://identity.ic0.app";
  const issuerOrigin = "https://metaissuer.vc/";
  const relyingPartyOrigin = "https://relyingparty.vc/";
  // Source: II
  const VcFlowReady = {
    jsonrpc: "2.0",
    method: "vc-flow-ready",
  };
  const vcVerifiablePresentationMessage = {
    id: "1",
    jsonrpc: "2.0",
    verifiablePresentation: "12345",
  };

  beforeEach(() => {
    window.open = vi.fn();
  })
  
  it("calls onSuccess with a valid ", async () => {
    new Promise<void>((done) => {
      const onError = vi.fn();
      requestVerifiablePresentation({
        onSuccess: () => {
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
      window.postMessage(
        vcVerifiablePresentationMessage,
        relyingPartyOrigin,
      );
    })
  });
});
