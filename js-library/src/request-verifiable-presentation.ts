export type CredentialData = {
  credentialSpec: CredentialSpec;
  credentialSubject: string;
}

export type CredentialSpec = {
  credentialType: string;
  arguments: Record<string, string | number>;
};

export type IssuerData = {
  origin: string;
  canisterId?: string;
};

let iiWindow: Window | null = null;
let nextFlowId = 0;

export const requestVerifiablePresentation = ({
  onSuccess,
  onError,
  credentialData: { credentialSpec, credentialSubject },
  issuerData,
  windowOpenerFeatures,
  derivationOrigin,
  identityProvider,
}: {
  onSuccess: (verifiablePresentation: string) => void | Promise<void>;
  onError: (err?: string) => void | Promise<void>;
  credentialData: CredentialData;
  issuerData: IssuerData;
  windowOpenerFeatures?: string;
  derivationOrigin: string | undefined;
  identityProvider: string;
}) => {
  nextFlowId += 1;
  const startFlow = (evnt: MessageEvent) => {
    const req = {
      id: String(nextFlowId),
      jsonrpc: "2.0",
      method: "request_credential",
      params: {
        issuer: issuerData,
        credentialSpec,
        credentialSubject,
        derivationOrigin: derivationOrigin,
      },
    };
    window.addEventListener("message", handleFlowFinished);
    window.removeEventListener("message", handleFlowReady);
    evnt.source?.postMessage(req, { targetOrigin: evnt.origin });
  };
  const finishFlow = async (evnt: MessageEvent) => {
    try {
      if (evnt.data?.error !== undefined) {
        throw new Error(evnt.data.error);
      }
      // Make the presentation presentable
      const verifiablePresentation = evnt.data?.result?.verifiablePresentation;
      if (verifiablePresentation === undefined) {
        // This should never happen
        onError("Couldn't get the verifiable credential");
      } else {
        onSuccess(verifiablePresentation);
      }
    } catch (err) {
      onError(`Error getting the verifiable credential: ${err}`);
    } finally {
      iiWindow?.close();
      window.removeEventListener("message", handleFlowFinished);
    }
  };
  const handleFlowFinished = (evnt: MessageEvent) => {
    if (evnt.data?.method === "vc-flow-ready") {
      startFlow(evnt);
    } else if (evnt.data?.id === String(nextFlowId)) {
      finishFlow(evnt);
    }
  };
  const handleFlowReady = (evnt: MessageEvent) => {
    if (evnt.data?.method !== "vc-flow-ready") {
      return;
    }
    startFlow(evnt);
  };
  window.addEventListener("message", handleFlowReady);
  const url = new URL(identityProvider);
  url.pathname = "vc-flow/";
  iiWindow = window.open(url, "_blank", windowOpenerFeatures);
};
