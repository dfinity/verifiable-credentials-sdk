/**
 * This module provides a function to request a verifiable presentation to an issuer through an Identity Provider.
 *
 * More info about the flow: https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md
 *
 * There is only one function exposed: `requestVerifiablePresentation`.
 */
import type { Principal } from "@icp-sdk/core/principal";
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
  credentialSubject: Principal;
};
export type IssuerData = {
  origin: string;
  canisterId: Principal;
};
const VC_REQUEST_METHOD = "request_credential";
const VC_START_METHOD = "vc-flow-ready";
const JSON_RPC_VERSION = "2.0";
export type CredentialsRequest = {
  id: FlowId;
  jsonrpc: typeof JSON_RPC_VERSION;
  method: typeof VC_REQUEST_METHOD;
  params: {
    issuer: {
      origin: string;
      canisterId: string;
    };
    credentialSpec: CredentialRequestSpec;
    credentialSubject: string;
    derivationOrigin?: string;
  };
};
/**
 * Output types
 */
type VerifiablePresentation = string;
export type VerifiablePresentationResponse =
  | { Ok: VerifiablePresentation }
  | { Err: string };
export type OnSuccessCallback = (
  verifiablePresentation: VerifiablePresentationResponse,
) => void | Promise<void>;
export type OnErrorCallback = (err?: string) => void | Promise<void>;

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
      issuer: {
        origin: issuerData.origin,
        canisterId: issuerData.canisterId?.toText(),
      },
      credentialSpec,
      credentialSubject: credentialSubject.toText(),
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

export type RequestVerifiablePresentationParams = {
  onSuccess: OnSuccessCallback;
  onError: (err?: string) => void | Promise<void>;
  credentialData: CredentialRequestData;
  issuerData: IssuerData;
  windowOpenerFeatures?: string;
  derivationOrigin?: string;
  identityProvider: URL;
};

/**
 * Function to request a verifiable presentation to an issuer through an Identity Provider.
 *
 * Summary of the flow:
 * - Open a new window or tab with the Identity Provider.
 * - Wait for a window post message from the Identity Provider.
 * - Send a request to the Identity Provider through the window post message.
 * - Wait for the response from the Identity Provider.
 * - Call `onSuccess` callback when the flow was successful. Not necessarily that the credential was received.
 * - Call `onError` callback when the flow has some technical error or the user closes the window.
 *
 * @param {RequestVerifiablePresentationParams} params
 * @param {OnSuccessCallback} params.onSuccess - Callback function that is called when the flow with the Identity Provider is successful.
 * It receives either the verifiable presentation or an message that the credential was not received.
 * The message doesn't expose different errors to keep the privacy of the user.
 * @param {OnErrorCallback} params.onError - Callback function that is called when the flow has some technical error or the user closes the window.
 * @param {CredentialRequestData} params.credentialData - Data to request the verifiable credential.
 * @param {IssuerData} params.issuerData - Data of the issuer.
 * @param {string} params.windowOpenerFeatures - Features of the window that opens the Identity Provider.
 *   @example "toolbar=0,location=0,menubar=0,width=500,height=500,left=100,top=100"
 * @param {string} params.derivationOrigin - Indicates an origin that should be used for principal derivation.
 * It's the same value as the one used when logging in.
 * More info: https://internetcomputer.org/docs/current/references/ii-spec/#alternative-frontend-origins
 * @param {string} params.identityProvider - URL of the Identity Provider.
 * @returns {void}
 */
export const requestVerifiablePresentation = ({
  onSuccess,
  onError,
  credentialData,
  issuerData,
  windowOpenerFeatures,
  derivationOrigin,
  identityProvider,
}: RequestVerifiablePresentationParams): void => {
  const handleFlowFactory = (currentFlowId: FlowId) => (evnt: MessageEvent) => {
    // We convert the origin to URL type to avoid problems with trailing slashes
    // when comparing it with the identityProvider.
    let evntOriginUrl: URL | undefined;
    try {
      evntOriginUrl = new URL(evnt.origin);
    } catch (err: unknown) {
      console.warn(
        `WARNING: expected origin to be URL, got '${evnt.origin} instead' (ignoring)`,
      );
      return;
    }
    // The handler is listening to all window messages.
    // For example, a browser extension could send messages that we want to ignore.
    if (evntOriginUrl?.origin !== identityProvider.origin) {
      console.warn(
        `WARNING: expected origin '${identityProvider}', got '${evntOriginUrl.origin}' (ignoring)`,
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
      evnt.source?.postMessage(request, { targetOrigin: evntOriginUrl.origin });
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
  let url;
  try {
    url = new URL(identityProvider);
  } catch (err) {
    onError("The parameter `identityProvider` must be a valid URL.");
    return;
  }
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
