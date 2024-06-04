import { AuthClient } from "@dfinity/auth-client";
import { decodeJwt } from "jose";
import {
  requestVerifiablePresentation,
  type VerifiablePresentationResponse,
} from "@dfinity/verifiable-credentials/request-verifiable-presentation";
import { Principal } from "@dfinity/principal";

const vcContainer = document.getElementById("vc-container");
const loggedInContainer = document.getElementById("logged-in");
const hideLoggedIn = () => {
  loggedInContainer?.classList.add("hidden");
  vcContainer?.classList.add("hidden");
};

const showLoggedIn = () => {
  loggedInContainer?.classList.remove("hidden");
};

const showVcContainer = () => {
  vcContainer?.classList.remove("hidden");
};

const hideNotLoggedIn = () => {
  document.getElementById("not-logged-in")?.classList.add("hidden");
};

let iiURL: string | null = null;

hideLoggedIn();

const loginButton = document.getElementById("login");
const requesetCredentialForm =
  (document.getElementById("request-credential") as HTMLFormElement) || null;
const userPrincipal = document.getElementById("user-principal");
const addArgumentButton = document.getElementById("add-argument");
const authClient = await AuthClient.create();
loginButton?.addEventListener("click", async () => {
  const iiURLElement = document.getElementById(
    "ii-url"
  ) as HTMLInputElement | null;
  if (iiURLElement) {
    iiURL = iiURLElement.value;
    await authClient.login({
      identityProvider: iiURL,
      onSuccess: () => {
        showLoggedIn();
        hideNotLoggedIn();
        if (userPrincipal) {
          userPrincipal.innerText = `User principal: ${authClient
            .getIdentity()
            .getPrincipal()
            .toText()}`;
        }
      },
      onError: (error) => {
        console.error(error);
      },
    });
  } else {
    alert("Please provide an Identity Provider URL");
  }
});

const credentialArguments: Record<string, string | number> = {};

addArgumentButton?.addEventListener("click", () => {
  const argumentsContainer = document.getElementById("arguments");
  if (argumentsContainer) {
    const key = document.getElementById("argument-key") as HTMLInputElement;
    const value = document.getElementById("argument-value") as HTMLInputElement;
    const argType = document.getElementById(
      "argument-type"
    ) as HTMLSelectElement;
    if (key && value && argType) {
      credentialArguments[key.value] =
        argType.value === "number" ? Number(value.value) : value.value;
      key.value = "";
      value.value = "";
    }
    argumentsContainer.innerHTML = "";
    const argumentTemplate = document.getElementById("arguments-template");
    Object.entries(credentialArguments).forEach(([key, value]) => {
      if (argumentTemplate) {
        const newArgument = argumentTemplate.cloneNode(true) as HTMLElement;
        newArgument.id = "";
        const keyElement = newArgument.querySelector(".key");
        const valueElement = newArgument.querySelector(".value");
        const typeElement = newArgument.querySelector(".type");
        if (keyElement && valueElement && typeElement) {
          keyElement.innerHTML = `${key}: `;
          valueElement.innerHTML = `${String(value)} - `;
          typeElement.innerHTML = `type: ${typeof value}`;
        }
        newArgument.classList.remove("hidden");
        argumentsContainer.appendChild(newArgument);
      }
    });
  }
});

requesetCredentialForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const formData = new FormData(requesetCredentialForm);
  const issuerUrl = formData.get("issuer-url") as string;
  const credentialType = formData.get("credential-type") as string;
  requestCredentials({
    issuerUrl,
    credentialType,
    credentialArgs: credentialArguments,
  });
});

const requestCredentials = async ({
  issuerUrl,
  canisterId,
  derivationOrigin,
  credentialType,
  credentialArgs,
}: {
  issuerUrl: string;
  canisterId?: Principal;
  derivationOrigin?: string;
  credentialType: string;
  credentialArgs: Record<string, string | number>;
}) => {
  // We shouldn't happen because we don't show the form to request credentials
  // until the user is logged in.
  if (!iiURL) {
    alert("Please login first");
    return;
  }
  const identity = authClient.getIdentity();
  const principal = identity.getPrincipal().toText();
  requestVerifiablePresentation({
    onSuccess: async (
      verifiablePresentation: VerifiablePresentationResponse
    ) => {
      showVcContainer();
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
      origin: issuerUrl,
      canisterId: canisterId?.toText(),
    },
    credentialData: {
      credentialSpec: {
        credentialType,
        arguments: credentialArgs,
      },
      credentialSubject: principal,
    },
    identityProvider: iiURL,
    derivationOrigin,
  });
};
