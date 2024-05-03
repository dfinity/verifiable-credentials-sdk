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
const idpWindows: Map<FlowId, Window | null> = new Map();
const createFlowId = (): FlowId => nanoid();

type FlowId = string;
/**
 * State Machine of the flow:
 *    /<-------------------------------------------- Identity Provider closes window -----------------------------------------------------\
 *   |<-- User closes window ----\<--------------------------------\<--------------------------------\                                    |
 *   v                           |                                 |                                 |                                    |
 * (Off) -- open window --> (ongoing) -- receive ready msg --> (ongoing) -- send request msg --> (ongoing) -- receive response msg --> (finalized)
 *
 * We care about how the window is closed because in case of the user closing the window, we want to call `onError` with `ERROR_USER_INTERRUPT`.
 * But we can't listen to the event of the user closing the window. The only way we know about it is by checking at intervals whether the window is closed.
 */
type Status = "initialized" | "started" | "ongoing" | "finalized";
const currentFlows = new Map<FlowId, Status>();

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
  evnt.data?.method === VC_START_METHOD &&
  currentFlows.get(flowId) === "initialized";

// As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#3-get-a-response
const isKnownFlowMessage = ({
  evnt,
  flowId,
}: {
  evnt: MessageEvent;
  flowId: FlowId;
}): boolean =>
  currentFlows.get(evnt.data?.id) === "ongoing" && evnt.data?.id === flowId;

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
      currentFlows.set(currentFlowId, "started");
      const request = createCredentialRequest({
        derivationOrigin,
        issuerData,
        credentialData,
        nextFlowId: currentFlowId,
      });
      evnt.source?.postMessage(request, { targetOrigin: evnt.origin });
      currentFlows.set(nextFlowId, "ongoing");
      return;
    }

    if (isKnownFlowMessage({ evnt, flowId: currentFlowId })) {
      try {
        currentFlows.set(evnt.data.id, "finalized");
        const credentialResponse = getCredentialResponse(evnt);
        onSuccess(credentialResponse);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : JSON.stringify(err);
        onError(`Error getting the verifiable credential: ${message}`);
      } finally {
        currentFlows.delete(currentFlowId);
        idpWindows.get(currentFlowId)?.close();
        idpWindows.delete(currentFlowId);
        window.removeEventListener("message", handleCurrentFlow);
      }
      return;
    }

    console.warn(
      `WARNING: unexpected message: ${JSON.stringify(evnt.data)} (ignoring)`,
    );
  };
  const nextFlowId = createFlowId();
  currentFlows.set(nextFlowId, "initialized");
  const handleCurrentFlow = handleFlowFactory(nextFlowId);
  window.addEventListener("message", handleCurrentFlow);
  const url = new URL(identityProvider);
  url.pathname = "vc-flow/";
  // As defined in the spec: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#1-load-ii-in-a-new-window
  const idpWindow = window.open(url, "idpWindow", windowOpenerFeatures);
  if (idpWindow !== null) {
    idpWindows.set(nextFlowId, idpWindow);
    // Check if the _idpWindow is closed by user.
    const checkInterruption = (flowId: FlowId): void => {
      // The _idpWindow is opened and not yet closed by the client
      if (
        idpWindow.closed &&
        currentFlows.has(flowId) &&
        currentFlows.get(flowId) !== "finalized"
      ) {
        currentFlows.delete(flowId);
        idpWindows.delete(flowId);
        window.removeEventListener("message", handleCurrentFlow);
        onError(ERROR_USER_INTERRUPT);
      } else if (idpWindows.has(flowId)) {
        setTimeout(() => checkInterruption(flowId), INTERRUPT_CHECK_INTERVAL);
      }
    };
    checkInterruption(nextFlowId);
  }
};
