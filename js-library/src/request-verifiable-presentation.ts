import { decodeJwt, type JWTPayload } from "jose";

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

export type CredentialParameters = Record<
  string,
  Record<string, string | number>
>;

export type VerifiableCredential = JWTPayload & {
  vc: {
    // TODO: Confirm. II says `string` but the spec says `string[]`.
    "@context": string | string[];
    credentialSubject: CredentialParameters;
    type: string[];
  };
};

// Type got from decoding the JWT
// Source: https://www.w3.org/TR/vc-data-model/#example-jwt-payload-of-a-jwt-based-verifiable-presentation-non-normative
type VerifiableCredentialJwtClaims = {
  // TODO: Confirm. II says `string` but the spec says `string[]`.
  "@context": string | string[];
  // TODO: Confirm. II says `string` but the spec says `string[]` or `[string]`.
  type: "VerifiablePresentation";
  verifiableCredential: [string, string];
};

type VerifiableCredentialClaimsDecoded = VerifiableCredentialJwtClaims & {
  identityAliasIdCredential: VerifiableCredential;
  subjectVerifiableCredential: VerifiableCredential;
};

export type VerifiablePresentationDecoded = JWTPayload & {
  vp: VerifiableCredentialClaimsDecoded;
};

type VerifiablePresentationJwtData = JWTPayload & {
  vp: VerifiableCredentialJwtClaims;
};

export type VerifiablePresentationSuccess = {
  verifiablePresentation: string;
  decodedJwt: VerifiablePresentationDecoded;
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

export const decodeCredentials = (
  verifiablePresentation: string,
): VerifiablePresentationDecoded => {
  const decodedJwt = decodeJwt<VerifiablePresentationJwtData>(
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
      decodedJwt: decodedCredentials,
    };
  } catch (err) {
    throw new Error(`Error decoding the verifiable presentation: ${err}`);
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
