# SDK For Verifiable Credentials in Internet Computer

This repository contains a set of libraries to manage verifiable credentials whether you are an issuer or a relying party.

## Libraries

- [Javascript Library](./js-library/README.md). At the moment only a client for relying parties.
- [ic-verifiable-credentials](./rust-packages/ic-verifiable-credentials/README.md). Verifiable credentials issuing and verification for IC canisters.

## Projects

- [Dummy Relying Party](./dummy-relying-party/README.md). A canister that provides a dummy relying party to request any kind of credentials.
- [Dummy Issuer](./dummy-issuer/README.md). A canister is a dummy issuer to issue any kind of credentials.

### Test Projects in Mainnet

There is a deployed version in Internet Computer here: [https://l7rua-raaaa-aaaap-ahh6a-cai.icp0.io/](https://l7rua-raaaa-aaaap-ahh6a-cai.icp0.io/).

You can test it against the dummy issuer deployed in [canister id qdiif-2iaaa-aaaap-ahjaq-cai](https://dashboard.internetcomputer.org/canister/qdiif-2iaaa-aaaap-ahjaq-cai).

You can use the staging Internet Identity: [https://fgte5-ciaaa-aaaad-aaatq-cai.ic0.app/](https://fgte5-ciaaa-aaaad-aaatq-cai.ic0.app/).
