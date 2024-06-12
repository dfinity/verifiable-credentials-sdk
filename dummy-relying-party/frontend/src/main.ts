import "./style.css";
import { AuthClient } from "@dfinity/auth-client";
import { decodeJwt } from "jose";
import {
  requestVerifiablePresentation,
  type VerifiablePresentationResponse,
} from "@dfinity/verifiable-credentials/request-verifiable-presentation";
import { Principal } from "@dfinity/principal";

let iiURL: URL | null = null;
let authClient: AuthClient | null = await AuthClient.create();

/**
 * LOGIN LOGIC
 */
const loginDataElement = document.getElementById("login-data");
const logInForm = document.getElementById("log-in-form");
const iiURLElement = document.getElementById(
  "ii-url"
) as HTMLInputElement | null;

const showCredentialsInputs = () => {
  logInForm?.classList.add("hidden");
  requestCredentialForm?.classList.remove("hidden");
  requestCredentialForm?.classList.add("flex");
};

// Login the user into the provided II URL.
logInForm?.addEventListener("submit", async (evt) => {
  evt.preventDefault();
  if (iiURLElement) {
    authClient = await AuthClient.create();
    try {
      iiURL = new URL(iiURLElement.value);
    } catch (e) {
      alert("Please provide a valid Identity Provider URL");
      return;
    }
    await authClient.login({
      identityProvider: iiURL,
      onSuccess: () => {
        showCredentialsInputs();
        if (loginDataElement) {
          loginDataElement.classList.remove("hidden");
          loginDataElement.classList.add("flex");
          loginDataElement.innerHTML = `
          <p><span class="font-bold">Identity provider:</span> ${iiURL}</p>
          <p><span class="font-bold">User principal:</span> ${authClient
            ?.getIdentity()
            .getPrincipal()
            .toText()}</p>`;
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

/**
 * REQUEST CREDENTIAL LOGIC
 */
const addArgumentForm = document.getElementById("add-argument-form");
const vcContainer = document.getElementById("vc-container");
const requestCredentialForm = document.getElementById(
  "request-credential-form"
);
const requestCredentialButton = document.getElementById(
  "request-credential-button"
);
const credentialSpecElement = document.getElementById("credential-spec");
const credentialTypeElement = document.getElementById("credential-type");
const issuerUrlElement = document.getElementById(
  "issuer-url"
) as HTMLInputElement;
const canisterIdElement = document.getElementById(
  "issuer-canister-id"
) as HTMLInputElement;

const showVcContainer = () => {
  vcContainer?.classList.remove("hidden");
  vcContainer?.classList.remove("sm:hidden");
  vcContainer?.classList.add("sm:grid");
};

const hideVcContainer = () => {
  vcContainer?.classList.add("hidden");
  vcContainer?.classList.add("sm:hidden");
  vcContainer?.classList.remove("sm:grid");
};

type CredentialSpec = {
  credentialType: string;
  arguments: Record<string, string | number>;
};
const credentialSpec: CredentialSpec = {
  credentialType: "",
  arguments: {},
};

// This function renders the credential spec in JSON format.
const renderCredentialSpec = () => {
  if (credentialSpecElement) {
    credentialSpecElement.innerHTML = JSON.stringify(credentialSpec, null, 2);
  }
};

// Render the initial credential spec which is empty.
renderCredentialSpec();

// Watch for changes in the credential type select element.
credentialTypeElement?.addEventListener("change", (event) => {
  const credentialType = (event.target as HTMLSelectElement).value;
  credentialSpec.credentialType = credentialType;
  renderCredentialSpec();
});

// Watch for new arguments added to the credential spec.
addArgumentForm?.addEventListener("submit", (evt) => {
  evt.preventDefault();
  const key = document.getElementById("argument-key") as HTMLInputElement;
  const value = document.getElementById("argument-value") as HTMLInputElement;
  const argType = document.getElementById("argument-type") as HTMLSelectElement;
  if (key && value && argType) {
    credentialSpec.arguments[key.value] =
      argType.value === "number" ? Number(value.value) : value.value;
    key.value = "";
    value.value = "";
    renderCredentialSpec();
  }
});

requestCredentialButton?.addEventListener("click", async () => {
  let canisterId: Principal;
  try {
    canisterId = Principal.fromText(canisterIdElement.value);
  } catch (e) {
    alert("Please provide a valid canister ID");
    return;
  }
  requestCredentials({
    issuerUrl: issuerUrlElement.value,
    credentialSpec,
    canisterId,
  });
});

/**
 * DECODE CREDENTIAL LOGIC
 */
const credentialErrorElement = document.getElementById("credential-error");
const vcResultElement = document.getElementById("vc-result");
const decodeCredentialPresentationButton = document.getElementById(
  "decode-credential-presentation-button"
);
const copyCredentialPresentationButton = document.getElementById(
  "copy-credential-presentation-button"
);
const decodedCredentialPresentation =
  document.getElementById("vc-presentation");
const decodedCredentialPresentationContainer = document.getElementById(
  "vc-presentation-container"
);
const decodedAliasContainer = document.getElementById("vc-alias-container");
const decodedAliasElement = document.getElementById("vc-alias");
const decodeAliasButton = document.getElementById("decode-alias-button");
const decodeCredentialButton = document.getElementById(
  "decode-credential-button"
);
const copyAliasButton = document.getElementById("copy-alias-button");
const copyCredentialButton = document.getElementById("copy-credential-button");
const decodedCredentialContainer = document.getElementById(
  "vc-credential-container"
);
const decodedCredentialElement = document.getElementById("vc-credential");

// Functions to render the decoded credential.
const renderDecodedCredential = (jwt: string) => {
  if (decodedCredentialContainer && decodedCredentialElement) {
    decodedCredentialContainer.classList.remove("hidden");
    decodedCredentialContainer.classList.add("flex");
    decodedCredentialElement.innerText = JSON.stringify(
      decodeJwt(jwt),
      null,
      2
    );
    window.scrollTo({
      top: decodedCredentialContainer.offsetTop,
      behavior: "smooth",
    });
  }
};

const renderDecodedAlias = (jwt: string) => {
  if (decodedAliasContainer && decodedAliasElement) {
    decodedAliasContainer.classList.remove("hidden");
    decodedAliasContainer.classList.add("flex");
    decodedAliasElement.innerText = JSON.stringify(decodeJwt(jwt), null, 2);
    window.scrollTo({
      top: decodedAliasContainer.offsetTop,
      behavior: "smooth",
    });
  }
};

const renderDecodedCredentialPresentation = (jwt: string) => {
  if (decodedCredentialPresentation && decodedCredentialPresentationContainer) {
    const decodedPresentation = decodeJwt(jwt) as any;
    decodedCredentialPresentationContainer.classList.remove("hidden");
    decodedCredentialPresentationContainer.classList.add("flex");
    decodedCredentialPresentation.innerText = JSON.stringify(
      decodedPresentation,
      null,
      2
    );
    window.scrollTo({
      top: decodedCredentialPresentationContainer.offsetTop,
      behavior: "smooth",
    });

    const [alias, credential] = decodedPresentation.vp.verifiableCredential;
    decodeAliasButton?.addEventListener("click", () => {
      renderDecodedAlias(alias);
    });
    decodeCredentialButton?.addEventListener("click", () => {
      renderDecodedCredential(credential);
    });
    copyAliasButton?.addEventListener("click", () => {
      navigator.clipboard.writeText(alias);
    });
    copyCredentialButton?.addEventListener("click", () => {
      navigator.clipboard.writeText(credential);
    });
  }
};

const renderCredential = (jwt: string) => {
  showVcContainer();
  if (credentialErrorElement) {
    credentialErrorElement.innerText = "";
  }
  if (vcResultElement && vcContainer) {
    vcResultElement.innerText = jwt;
    window.scrollTo({ top: vcContainer.offsetTop, behavior: "smooth" });
  }

  decodeCredentialPresentationButton?.addEventListener("click", () => {
    renderDecodedCredentialPresentation(jwt);
  });

  copyCredentialPresentationButton?.addEventListener("click", () => {
    navigator.clipboard.writeText(jwt);
  });
};

const renderError = (error: string | undefined) => {
  console.log("Error obtaining credential", error);
  hideVcContainer();
  if (credentialErrorElement) {
    credentialErrorElement.innerText = `There was an error obtaining the credential: ${error}`;
  }
};

// Request credentials from the Issuer.
const requestCredentials = async ({
  issuerUrl,
  canisterId,
  derivationOrigin,
  credentialSpec,
}: {
  issuerUrl: string;
  canisterId: Principal;
  derivationOrigin?: string;
  credentialSpec: CredentialSpec;
}) => {
  // We shouldn't happen because we don't show the form to request credentials
  // until the user is logged in.
  if (!iiURL || !authClient) {
    alert("Please login first");
    return;
  }
  const principal = authClient.getIdentity().getPrincipal();
  requestVerifiablePresentation({
    onSuccess: async (
      verifiablePresentation: VerifiablePresentationResponse
    ) => {
      if ("Ok" in verifiablePresentation) {
        renderCredential(verifiablePresentation.Ok);
      } else {
        renderError(verifiablePresentation.Err);
      }
    },
    onError: renderError,
    issuerData: {
      origin: issuerUrl,
      canisterId,
    },
    credentialData: {
      credentialSpec,
      credentialSubject: principal,
    },
    identityProvider: new URL(iiURL),
    derivationOrigin,
  });
};
