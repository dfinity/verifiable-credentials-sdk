import { nanoid } from "nanoid";

/**
 * Helper types.
 */
type CredentialsArguments = Record<string, string | number>;
type CredentialType = string;
/**
 * Types used to request the verifiable presentation.
 */
export type CredentialRequestSpec = {
  credentialType: CredentialType;
  arguments: CredentialsArguments;
};
export type CredentialRequestData = {
  credentialSpec: CredentialRequestSpec;
  credentialSubject: string;
};
export type IssuerData = {
  origin: string;
  canisterId?: string;
};
const VC_REQUEST_METHOD = "request_credential";
const VC_START_METHOD = "vc-flow-ready";
const JSON_RPC_VERSION = "2.0";
export type CredentialsRequest = {
  id: FlowId;
  jsonrpc: typeof JSON_RPC_VERSION;
  method: typeof VC_REQUEST_METHOD;
  params: {
    issuer: IssuerData;
    credentialSpec: CredentialRequestSpec;
    credentialSubject: string;
    derivationOrigin: string | undefined;
  };
};

/**
 * Helper functions
 */
const iiWindows: Map<FlowId, Window | null> = new Map();
const createFlowId = (): FlowId => nanoid();

type FlowId = string;
const currentFlows = new Set<FlowId>();

// As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#2-request-a-vc
const createCredentialRequest = ({
  issuerData,
  derivationOrigin,
  credentialData: { credentialSpec, credentialSubject },
  nextFlowId,
}: {
  issuerData: IssuerData;
  derivationOrigin: string | undefined;
  credentialData: CredentialRequestData;
  nextFlowId: FlowId;
}): CredentialsRequest => {
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

const getCredential = (evnt: MessageEvent): string => {
  if (evnt.data?.error !== undefined) {
    // TODO: Return this error in onSuccess, not onError.
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

const isJSONRPC = (evnt: MessageEvent): boolean => {
  return evnt.data?.jsonrpc === JSON_RPC_VERSION;
};

// As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#1-load-ii-in-a-new-window
const isExpectedNotification = ({
  evnt,
  flowId,
}: {
  evnt: MessageEvent;
  flowId: FlowId;
}): boolean =>
  evnt.data?.method === VC_START_METHOD && !currentFlows.has(flowId);

// As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#3-get-a-response
const isKnownFlowMessage = ({
  evnt,
  flowId,
}: {
  evnt: MessageEvent;
  flowId: FlowId;
}): boolean => currentFlows.has(evnt.data?.id) && evnt.data?.id === flowId;

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
  credentialData: CredentialRequestData;
  issuerData: IssuerData;
  windowOpenerFeatures?: string;
  derivationOrigin: string | undefined;
  identityProvider: string;
}) => {
  const handleFlowFactory = (currentFlowId: FlowId) => (evnt: MessageEvent) => {
    // The handler is listening to all window messages.
    // For example, a browser extension could send messages that we want to ignore.
    if (evnt.origin !== identityProvider) {
      console.warn(
        `WARNING: expected origin '${identityProvider}', got '${evnt.origin}' (ignoring)`,
      );
      return;
    }

    // As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#interaction-model
    if (!isJSONRPC(evnt)) {
      console.warn(
        `WARNING: expected JSON-RPC message, got '${JSON.stringify(evnt.data)}' (ignoring)`,
      );
      return;
    }

    if (isExpectedNotification({ evnt, flowId: currentFlowId })) {
      const request = createCredentialRequest({
        derivationOrigin,
        issuerData,
        credentialData,
        nextFlowId: currentFlowId,
      });
      currentFlows.add(request.id);
      evnt.source?.postMessage(request, { targetOrigin: evnt.origin });
      return;
    }

    if (isKnownFlowMessage({ evnt, flowId: currentFlowId })) {
      try {
        const credential = getCredential(evnt);
        onSuccess(credential);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : JSON.stringify(err);
        onError(`Error getting the verifiable credential: ${message}`);
      } finally {
        currentFlows.delete(evnt.data.id);
        iiWindows.get(currentFlowId)?.close();
        iiWindows.delete(currentFlowId);
        window.removeEventListener("message", handleCurrentFlow);
      }
      return;
    }

    console.warn(
      `WARNING: unexpected message: ${JSON.stringify(evnt.data)} (ignoring)`,
    );
  };
  const nextFlowId = createFlowId();
  const handleCurrentFlow = handleFlowFactory(nextFlowId);
  // TODO: Check if user closed the window and return an error.
  // Check how AuthClient does it: https://github.com/dfinity/agent-js/blob/a51bd5b837fd5f98daca5a45dfc4a060a315e62e/packages/auth-client/src/index.ts#L489
  window.addEventListener("message", handleCurrentFlow);
  const url = new URL(identityProvider);
  url.pathname = "vc-flow/";
  // As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#1-load-ii-in-a-new-window
  const iiWindow = window.open(url, "idpWindow", windowOpenerFeatures);
  if (iiWindow !== null) {
    iiWindows.set(nextFlowId, iiWindow);
  }
};
