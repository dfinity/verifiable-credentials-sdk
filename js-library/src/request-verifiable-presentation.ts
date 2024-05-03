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
 * Output types
 */
type VerifiablePresentation = string;
export type VerifiablePresentationResponse =
  | { Ok: VerifiablePresentation }
  | { Err: string };

/**
 * Helper functions
 */
const iiWindows: Map<FlowId, Window | null> = new Map();
const createFlowId = (): FlowId => nanoid();

type FlowId = string;
const currentFlows = new Set<FlowId>();

const INTERRUPT_CHECK_INTERVAL = 500;
export const ERROR_USER_INTERRUPT = "UserInterrupt";

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

const getCredentialResponse = (
  evnt: MessageEvent,
): VerifiablePresentationResponse => {
  if (evnt.data?.error !== undefined) {
    return { Err: evnt.data.error };
  }
  const verifiablePresentation = evnt.data?.result?.verifiablePresentation;
  if (verifiablePresentation === undefined) {
    throw new Error(
      `Key 'verifiablePresentation' not found in the message data: ${JSON.stringify(evnt.data)}`,
    );
  }
  return { Ok: verifiablePresentation };
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
  onSuccess: (
    verifiablePresentation: VerifiablePresentationResponse,
  ) => void | Promise<void>;
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
        // Identity Provider closes the window after sending the response.
        // We are checking in an interval whether the user closed the window.
        // Removing the flow from currentFlows prevents interpreting that the user interrupted the flow.
        // To check this in a test, I put `onSuccess` call inside a setTimeout
        // to simulate that handling took longer than the check for the user closing the window.
        // Then I advanced the time in the test and checked that `onSuccess` was called, and not `onError`.
        // The test "should not call onError when window closes after successful flow" was failing
        // if we didn't remove the curret flow id from currentFlows.
        currentFlows.delete(evnt.data.id);
        const credentialResponse = getCredentialResponse(evnt);
        onSuccess(credentialResponse);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : JSON.stringify(err);
        onError(`Error getting the verifiable credential: ${message}`);
      } finally {
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
  window.addEventListener("message", handleCurrentFlow);
  const url = new URL(identityProvider);
  url.pathname = "vc-flow/";
  // As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#1-load-ii-in-a-new-window
  const iiWindow = window.open(url, "idpWindow", windowOpenerFeatures);
  // Check if the _idpWindow is closed by user.
  const checkInterruption = (flowId: FlowId): void => {
    // The _idpWindow is opened and not yet closed by the client
    if (iiWindow) {
      if (iiWindow.closed && currentFlows.has(flowId)) {
        currentFlows.delete(flowId);
        iiWindows.delete(flowId);
        window.removeEventListener("message", handleCurrentFlow);
        onError(ERROR_USER_INTERRUPT);
      } else {
        setTimeout(() => checkInterruption(flowId), INTERRUPT_CHECK_INTERVAL);
      }
    }
  };
  checkInterruption(nextFlowId);
  if (iiWindow !== null) {
    iiWindows.set(nextFlowId, iiWindow);
  }
};
