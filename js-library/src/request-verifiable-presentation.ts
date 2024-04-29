export type CredentialData = {
  credentialSpec: CredentialSpec;
  credentialSubject: string;
};

export type CredentialSpec = {
  credentialType: string;
  arguments: Record<string, string | number>;
};

export type IssuerData = {
  origin: string;
  canisterId?: string;
};

const VC_REQUEST_METHOD = "request_credential";
const JSON_RPC_VERSION = "2.0";
type CredentialsRequest = {
  id: FlowId;
  jsonrpc: typeof JSON_RPC_VERSION;
  method: typeof VC_REQUEST_METHOD;
  params: {
    issuer: IssuerData;
    credentialSpec: CredentialSpec;
    credentialSubject: string;
    derivationOrigin: string | undefined;
  };
};

// Needed to reset the flow id between tests.
// TODO: Remove this when using UUIDs.
export const resetNextFlowId = () => {
  nextFlowIdCounter = 0;
};
// TODO: Support multiple flows at the same time.
let iiWindow: Window | null = null;
// TODO: Use UUIDs instead of incrementing integers.
let nextFlowIdCounter = 0;
const createFlowId = (): FlowId => {
  nextFlowIdCounter += 1;
  return String(nextFlowIdCounter);
};

type FlowId = string;
const currentFlows = new Set<FlowId>();

const createCredentialRequest = ({
  issuerData,
  derivationOrigin,
  credentialData: { credentialSpec, credentialSubject },
}: {
  issuerData: IssuerData;
  derivationOrigin: string | undefined;
  credentialData: CredentialData;
}): CredentialsRequest => {
  const nextFlowId = createFlowId();
  return {
    id: nextFlowId,
    jsonrpc: JSON_RPC_VERSION,
    method: VC_REQUEST_METHOD,
    params: {
      issuer: issuerData,
      credentialSpec,
      credentialSubject,
      derivationOrigin: derivationOrigin,
    },
  };
};

// TODO: Decode the verifiable presentation and return a typed object.
const getCredential = (evnt: MessageEvent): string => {
  if (evnt.data?.error !== undefined) {
    throw new Error(evnt.data.error);
  }
  const verifiablePresentation = evnt.data?.result?.verifiablePresentation;
  if (verifiablePresentation === undefined) {
    throw new Error(
      `Key 'verifiablePresentation' not found in the message data: ${JSON.stringify(evnt.data)}`,
    );
  }
  return verifiablePresentation;
};

export const requestVerifiablePresentation = ({
  onSuccess,
  onError,
  credentialData,
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
  const handleFlow = (evnt: MessageEvent) => {
    // TODO: Check if the message is from the identity provider
    // Check how AuthClient does it: https://github.com/dfinity/agent-js/blob/a51bd5b837fd5f98daca5a45dfc4a060a315e62e/packages/auth-client/src/index.ts#L504
    if (evnt.data?.method === "vc-flow-ready") {
      const request = createCredentialRequest({
        derivationOrigin,
        issuerData,
        credentialData,
      });
      currentFlows.add(request.id);
      evnt.source?.postMessage(request, { targetOrigin: evnt.origin });
    } else if (currentFlows.has(evnt.data?.id)) {
      try {
        const credential = getCredential(evnt);
        onSuccess(credential);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : JSON.stringify(err);
        onError(`Error getting the verifiable credential: ${message}`);
      } finally {
        currentFlows.delete(evnt.data.id);
        iiWindow?.close();
        window.removeEventListener("message", handleFlow);
      }
    }
  };
  // TODO: Check if user closed the window and return an error.
  // WARNING: We want to remove the id from `currentFlows` when the window is closed.
  // Check how AuthClient does it: https://github.com/dfinity/agent-js/blob/a51bd5b837fd5f98daca5a45dfc4a060a315e62e/packages/auth-client/src/index.ts#L489
  window.addEventListener("message", handleFlow);
  const url = new URL(identityProvider);
  url.pathname = "vc-flow/";
  iiWindow = window.open(url, "idpWindow", windowOpenerFeatures);
};
