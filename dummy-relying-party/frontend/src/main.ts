import { AuthClient } from "@dfinity/auth-client";
import { decodeJwt } from "jose";
import {
  requestVerifiablePresentation,
  type VerifiablePresentationResponse,
} from "@dfinity/verifiable-credentials/request-verifiable-presentation";

const II_URL = import.meta.env.VITE_INTERNET_IDENTITY_URL;
const ISSUER_ORIGIN = import.meta.env.VITE_ISSUER_ORIGIN;
const ISSUER_CANISTER_ID = import.meta.env.VITE_ISSUER_CANISTER_ID;
const loginButton = document.getElementById("login");
const vcContainer = document.getElementById("vc-container");
const earlyAdopterVCButton = document.getElementById("start-early-adopter-vc");
const attendeeVCForm =
  (document.getElementById("request-attendee-credential") as HTMLFormElement) ||
  null;
const loginStatus = document.getElementById("login-status");
const authClient = await AuthClient.create();
loginButton?.addEventListener("click", async () => {
  await authClient.login({
    identityProvider: II_URL,
    onSuccess: () => {
      loginButton?.classList.add("hidden");
      vcContainer?.classList.remove("hidden");
      if (loginStatus) {
        loginStatus.innerText = `Logged in as ${authClient
          .getIdentity()
          .getPrincipal()
          .toText()}`;
      }
    },
  });
});

earlyAdopterVCButton?.addEventListener("click", async () => {
  requestCredentials("EarlyAdopter", { sinceYear: 2024 });
});

attendeeVCForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const formData = new FormData(attendeeVCForm);
  const eventName = formData.get("event-name") as string;
  requestCredentials("EventAttendance", { eventName });
});

const requestCredentials = async (
  credentialType: string,
  credentialArgs: Record<string, string | number>
) => {
  const identity = authClient.getIdentity();
  const principal = identity.getPrincipal().toText();
  requestVerifiablePresentation({
    onSuccess: async (
      verifiablePresentation: VerifiablePresentationResponse
    ) => {
      const resultElement = document.getElementById("vc-result");
      const presentationElement = document.getElementById("vc-presentation");
      const aliasElement = document.getElementById("vc-alias");
      const credentialElement = document.getElementById("vc-credential");
      if ("Ok" in verifiablePresentation) {
        if (resultElement) {
          resultElement.innerText = verifiablePresentation.Ok;
        }
        const ver = decodeJwt(verifiablePresentation.Ok) as any;
        if (presentationElement) {
          presentationElement.innerText = JSON.stringify(ver, null, 2);
        }
        const creds = ver.vp.verifiableCredential;
        const [alias, credential] = creds.map((cred: string) =>
          JSON.stringify(decodeJwt(cred), null, 2)
        );
        if (aliasElement) {
          aliasElement.innerText = alias;
        }
        if (credentialElement) {
          credentialElement.innerText = credential;
        }
      } else {
        if (resultElement) {
          resultElement.innerText = "Credential not obtained";
        }
      }
    },
    onError() {
      const resultElement = document.getElementById("vc-result");
      if (resultElement) {
        resultElement.innerText =
          "There was an error obtaining the credential.";
      }
    },
    issuerData: {
      origin: ISSUER_ORIGIN,
      canisterId: ISSUER_CANISTER_ID,
    },
    credentialData: {
      credentialSpec: {
        credentialType,
        arguments: credentialArgs,
      },
      credentialSubject: principal,
    },
    identityProvider: II_URL,
    derivationOrigin: undefined,
  });
};
