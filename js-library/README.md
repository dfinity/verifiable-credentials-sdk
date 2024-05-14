# Javascript SDK For Verifiable Credentials

At the moment, this library only contains a function to interact with an Identity Provider to get a credential presentation for a user.

The flow to request a credential is performed through a browser.

## Installation

Install library

```bash
npm install @dfinity/verifiable-credentials
```

The bundle needs peer dependencies, be sure that following resources are available in your project as well.

```bash
npm install @dfinity/principal
```

Import per modules:

```javascript
// import * from '@dfinity/verifiable-credentials'; // Error: use sub-imports, to ensure small app size
import { requestVerifiablePresentation } from "@dfinity/verifiable-credentials/request-verifiable-presentation";
```

## Relying Party: Request Credentials

Use the function `requestVerifiablePresentation` to request credentials from an issuer.

### Summary

The function performs the following steps:

- Open a new window or tab with the Identity Provider.
- Wait for a window post message from the Identity Provider.
- Send a request to the Identity Provider through the window post message.
- Wait for the response from the Identity Provider.
- Call `onSuccess` callback when the flow was successful. Not necessarily that the credential was received.
- Call `onError` callback when the flow has some technical error or the user closes the window.

More info in the [Internet Identity Specification for Verifiable Credentials](https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md).

### Usage

To start a flow, call the function with the expected parameters:

```javascript
requestVerifiablePresentation({
  onSuccess: async (verifiablePresentation: VerifiablePresentationResponse) => {
    // Called when the flow finishes successfully.
  },
  onError() {
    // Called when there is a technical error.
  },
  issuerData: {
    origin: "<url of the origin>",
    canisterId: "<[optional] canister id>",
  },
  credentialData: {
    credentialSpec: {
      credentialType: '<credential type as expected by issuer>',
      arguments: {
        // Arguments to verify with the credential
      },
    },
    credentialSubject: "<user's principal>",
  },
  identityProvider: "<[optional] url identity provider>",
  derivationOrigin: "<[optional] origin for delegated identity>",
  windowOpenerFeatures: "<[optional] window opener config string>",
});
```

### Paremeters

List of properties expected in the parameter when calling the function:

- `onSuccess`: Function that will be called when the flow finishes. Not necessarily with the credential. This is also called if the user doesn't have the credential.
- `onError`: Function that will be called when the flow failes due to a technical error. Also when the user interrupts the flow.
- `issuerData`: Object with the `origin` and `canisterId` of the issuer.
- `credentialData`: Object with the subbject and the credential requested.
- `identityProvider`: URL of the Identity Provider. Ex: `"https://identity.ic0.app/"`.
- `derivationOrigin`: Indicates an origin that should be used for principal derivation. It's the same value as the one used when logging in. [More info](https://internetcomputer.org/docs/current/references/ii-spec/#alternative-frontend-origins).
- `windowOpenerFeatures` The flow will open a new window or tab. Pass here a configuration string to customize it.
