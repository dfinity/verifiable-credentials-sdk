import { decodeJwt, type JWTPayload } from "jose";

/**
 * Helper types.
 */
type CredentialsArguments = Record<string, string | number>;
type CredentialType = string;
type CredentialParameters = Record<CredentialType, CredentialsArguments>;
type CredentialContext = string | string[];
type EncryptedCredential = string;
type EncodedPresentation = string;

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
const JSON_RPC_VERSION = "2.0";
type CredentialsRequest = {
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
 * Types after decoding the JWT. Used internally.
 */
// Source: https://www.w3.org/TR/vc-data-model/#example-jwt-payload-of-a-jwt-based-verifiable-presentation-non-normative
type InternalVerifiableCredentialJwtClaims = {
  "@context": CredentialContext;
  // Source: https://github.com/dfinity/internet-identity/blob/e01fbd5ae2fd6fe1a2646c9b5d49f7e52b8810eb/src/frontend/src/flows/verifiableCredentials/index.ts#L452
  type: "VerifiablePresentation";
  verifiableCredential: [EncryptedCredential, EncryptedCredential];
};
type InternalVerifiablePresentationJwt = JWTPayload & {
  vp: InternalVerifiableCredentialJwtClaims;
};
type VerifiableCredential = JWTPayload & {
  vc: {
    "@context": CredentialContext;
    credentialSubject: CredentialParameters;
    type: string[];
  };
};

/**
 * Types used to return the decoded JWT.
 */
type VerifiableCredentialClaims = InternalVerifiableCredentialJwtClaims & {
  identityAliasIdCredential: VerifiableCredential;
  subjectVerifiableCredential: VerifiableCredential;
};
type VerifiablePresentation = JWTPayload & {
  vp: VerifiableCredentialClaims;
};
export type VerifiablePresentationSuccess = {
  verifiablePresentation: EncodedPresentation;
  decodedCredentials: VerifiablePresentation;
};

/**
 * Helper functions
 */

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
  credentialData: CredentialRequestData;
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

const decodeCredentials = (
  verifiablePresentation: string,
): VerifiablePresentation => {
  const decodedJwt = decodeJwt<InternalVerifiablePresentationJwt>(
    verifiablePresentation,
  );
  if (
    decodedJwt.vp.verifiableCredential === undefined ||
    decodedJwt.vp.verifiableCredential.length !== 2
  ) {
    throw new Error(`Verifiable credentials malformed ${decodeJwt}`);
  }
  const [alias, credential] = decodedJwt.vp.verifiableCredential.map(
    (encodedCredential: string) =>
      decodeJwt<VerifiableCredential>(encodedCredential),
  );

  return {
    ...decodedJwt,
    vp: {
      ...decodedJwt.vp,
      identityAliasIdCredential: alias,
      subjectVerifiableCredential: credential,
    },
  };
};

// TODO: Decode the verifiable presentation and return a typed object.
const getCredential = (evnt: MessageEvent): VerifiablePresentationSuccess => {
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
  try {
    const decodedCredentials = decodeCredentials(verifiablePresentation);
    return {
      verifiablePresentation,
      decodedCredentials: decodedCredentials,
    };
  } catch (err) {
    throw new Error(`Decoding credentials failed: ${err}`);
  }
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
  onSuccess: (
    verifiablePresentation: VerifiablePresentationSuccess,
  ) => void | Promise<void>;
  onError: (err?: string) => void | Promise<void>;
  credentialData: CredentialRequestData;
  issuerData: IssuerData;
  windowOpenerFeatures?: string;
  derivationOrigin: string | undefined;
  identityProvider: string;
}) => {
  const handleFlow = (evnt: MessageEvent) => {
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
  // Check how AuthClient does it: https://github.com/dfinity/agent-js/blob/a51bd5b837fd5f98daca5a45dfc4a060a315e62e/packages/auth-client/src/index.ts#L489
  window.addEventListener("message", handleFlow);
  const url = new URL(identityProvider);
  url.pathname = "vc-flow/";
  iiWindow = window.open(url, "idpWindow", windowOpenerFeatures);
};
