import "./style.css";
import { AuthClient } from "@dfinity/auth-client";
import { decodeJwt } from "jose";
import {
  requestVerifiablePresentation,
  type VerifiablePresentationResponse,
} from "@dfinity/verifiable-credentials/request-verifiable-presentation";
import { Principal } from "@dfinity/principal";

const vcContainer = document.getElementById("vc-container");
const logInForm = document.getElementById("log-in-form");
const requestCredentialForm = document.getElementById(
  "request-credential-form"
);
const requestCredentialButton = document.getElementById(
  "request-credential-button"
);

const showLoggedIn = () => {
  logInForm?.classList.remove("hidden");
};

const showVcContainer = () => {
  vcContainer?.classList.remove("hidden");
  vcContainer?.classList.remove("sm:hidden");
  vcContainer?.classList.add("sm:grid");
};

const showCredentials = () => {
  logInForm?.classList.add("hidden");
  requestCredentialForm?.classList.remove("hidden");
  requestCredentialForm?.classList.add("grid");
};

let iiURL: string | null = null;

const userPrincipal = document.getElementById("user-principal");
const addArgumentForm = document.getElementById("add-argument-form");
const authClient = await AuthClient.create();
logInForm?.addEventListener("submit", async (evt) => {
  evt.preventDefault();
  const iiURLElement = document.getElementById(
    "ii-url"
  ) as HTMLInputElement | null;
  if (iiURLElement) {
    iiURL = iiURLElement.value;
    await authClient.login({
      identityProvider: iiURL,
      onSuccess: () => {
        showLoggedIn();
        showCredentials();
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

type CredentialSpec = {
  credentialType: string;
  arguments: Record<string, string | number>;
};
const credentialSpec: CredentialSpec = {
  credentialType: "",
  arguments: {},
};

const renderCredentialSpec = () => {
  const credentialSpecElement = document.getElementById("credential-spec");
  if (credentialSpecElement) {
    credentialSpecElement.innerHTML = JSON.stringify(credentialSpec, null, 2);
  }
};

renderCredentialSpec();

document
  .getElementById("credential-type")
  ?.addEventListener("change", (event) => {
    const credentialType = (event.target as HTMLSelectElement).value;
    credentialSpec.credentialType = credentialType;
    renderCredentialSpec();
  });

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
  const issuerUrlElement = document.getElementById(
    "issuer-url"
  ) as HTMLInputElement;
  requestCredentials({
    issuerUrl: issuerUrlElement.value,
    credentialSpec,
  });
});

const renderDecodedCredential = (jwt: string) => {
  const container = document.getElementById("vc-credential-container");
  const element = document.getElementById("vc-credential");
  if (container && element) {
    container.classList.remove("hidden");
    container.classList.add("flex");
    element.innerText = JSON.stringify(decodeJwt(jwt), null, 2);
    window.scrollTo({ top: container.offsetTop, behavior: "smooth" });
  }
};

const renderDecodedAlias = (jwt: string) => {
  const aliasContainer = document.getElementById("vc-alias-container");
  const aliasElement = document.getElementById("vc-alias");
  if (aliasContainer && aliasElement) {
    aliasContainer.classList.remove("hidden");
    aliasContainer.classList.add("flex");
    aliasElement.innerText = JSON.stringify(decodeJwt(jwt), null, 2);
    window.scrollTo({ top: aliasContainer.offsetTop, behavior: "smooth" });
  }
};

const renderDecodedCredentialPresentation = (jwt: string) => {
  const presentationElement = document.getElementById("vc-presentation");
  const presentationContainer = document.getElementById(
    "vc-presentation-container"
  );
  if (presentationElement && presentationContainer) {
    const decodedPresentation = decodeJwt(jwt) as any;
    presentationContainer.classList.remove("hidden");
    presentationContainer.classList.add("flex");
    presentationElement.innerText = JSON.stringify(
      decodedPresentation,
      null,
      2
    );
    window.scrollTo({
      top: presentationContainer.offsetTop,
      behavior: "smooth",
    });

    const [alias, credential] = decodedPresentation.vp.verifiableCredential;
    document
      .getElementById("decode-alias-button")
      ?.addEventListener("click", () => {
        renderDecodedAlias(alias);
      });

    document
      .getElementById("decode-credential-button")
      ?.addEventListener("click", () => {
        renderDecodedCredential(credential);
      });

    document
      .getElementById("copy-alias-button")
      ?.addEventListener("click", () => {
        navigator.clipboard.writeText(alias);
      });

    document
      .getElementById("copy-credential-button")
      ?.addEventListener("click", () => {
        navigator.clipboard.writeText(credential);
      });
  }
};

const renderCredential = (jwt: string) => {
  showVcContainer();
  const resultElement = document.getElementById("vc-result");
  if (resultElement) {
    resultElement.innerText = jwt;
  }
  document
    .getElementById("decode-credential-presentation-button")
    ?.addEventListener("click", () => {
      renderDecodedCredentialPresentation(jwt);
    });

  document
    .getElementById("copy-credential-presentation-button")
    ?.addEventListener("click", () => {
      navigator.clipboard.writeText(jwt);
    });
};

const requestCredentials = async ({
  issuerUrl,
  canisterId,
  derivationOrigin,
  credentialSpec,
}: {
  issuerUrl: string;
  canisterId?: Principal;
  derivationOrigin?: string;
  credentialSpec: CredentialSpec;
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
      console.log("in onSuccess", verifiablePresentation);
      const resultElement = document.getElementById("vc-result");
      if ("Ok" in verifiablePresentation) {
        renderCredential(verifiablePresentation.Ok);
      } else {
        showVcContainer();
        if (resultElement) {
          resultElement.innerText = "Credential not obtained";
        }
      }
    },
    onError(err?: string) {
      console.log("Error obtaining credential", err);
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
      credentialSpec,
      credentialSubject: principal,
    },
    identityProvider: new URL(iiURL).origin,
    derivationOrigin,
  });
};
