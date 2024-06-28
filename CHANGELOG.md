# Upcoming

## Breaking Changes

## Improvements

- Add the credential arguments to the consent message of the dummy issuer.
- Add canister id input in the dummy relying party.
- Upgrade `@dfinity/verifiable-credentials` used in the dummy relying party to the latest published version.
- Port vc-util from II to this repository and rename it to `ic-verifiable-credentials`.

# 2024.06.10

Changes in the upcoming versions.

## New projects

- Dummy relying party.
- Dummy issuer.

## Breaking Changes

- Change the type of `CredentialData.credentialSubject` to `Principal`.
- Change the type of `IssuerData.canisterId` to `Principal` and make it mandatory.
- Change the type of `identityProvider` to `URL`.

## Non-breaking Changes

- `derivationOrigin` field is now optional.

## Installation

- List `@dfinity/principal` as a peer dependency.

# 2024.05.03

Release of the NPM library `@dfinity/verifiable-credentials`.

At the moment, this library only contains a function to interact with an Identity Provider to get a credential presentation for a user.
