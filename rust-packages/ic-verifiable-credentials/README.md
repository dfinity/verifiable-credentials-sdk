# ic-verifiable-credentials

## About

Verifiable credentials issuing and verification for IC canisters.

Issuers can use this library to issue credentials and verify the id alias credential received from the identity provider.

Relying parties can use this library to verify the credentials received.

More information about [Verifiable Credentials in the Internet Computer documentation](https://internetcomputer.org/docs/current/developer-docs/identity/verifiable-credentials/overview).

## Getting Started

### Installation

Install with cargo:

```shell
cargo add ic-verifiable-credentials
```

Or add it to your `Cargo.toml`.

```toml
ic-verifiable-credentials = "1.0.0"
```

### Usage

Main functions for issuers:

- `build_credential_jwt`. Builds a verifiable credential with the given parameters and returns the credential as a JWT-string.
- `did_for_principal`. Returns a DID for the given `principal`.
- `vc_jwt_to_jws`. Constructs and returns a JWS (a signed JWT) from the given components.
- `vc_signing_input`. Returns the effective bytes that will be signed when computing a canister signature for the given JWT-credential, verifiable via the specified public key.
- `vc_signing_input_hash`. Computes and returns SHA-256 hash of the given `signing_input`.
- `get_verified_id_alias_from_jws`. Verifies the given JWS-credential as an id_alias-VC and extracts the alias tuple.

Main function for relying parties:

- `validate_ii_presentation_and_claims`. Validates the provided presentation `vp_jwt`, both cryptographically and semantically.

## License

Distributed under the Apache License. See [LICENSE](https://github.com/dfinity/verifiable-credentials-sdk/blob/main/LICENSE) for more information.
