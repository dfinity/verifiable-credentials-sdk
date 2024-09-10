# Upcoming

Changes in the upcoming versions.

## Breaking Changes

## Improvements

# release-2024-09-10

## Breaking Changes

## Improvements

- Update dependency versions.
- Fix importing js-library in Webpack projects.

# release-2024-07-01

## New projects

- Add `ic-verifiable-credentials` crate to the repository.

## Improvements

- Add the credential arguments to the consent message of the dummy issuer.
- Add canister id input in the dummy relying party.
- Upgrade `@dfinity/verifiable-credentials` used in the dummy relying party to the latest published version.
- Port vc-util from II to this repository and rename it to `ic-verifiable-credentials`.
- Do not publish to NPM the JS library if there were no changes.
- Publish the dummy issuer and dummy relying party wasm modules and interface files as release artifacts so that they can be easily pulled into other projects.

# 2024.06.10

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
