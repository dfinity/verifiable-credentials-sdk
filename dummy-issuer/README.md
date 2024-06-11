# Dummy Issuer

This canister is a dummy issuer to issue any kind of credentials.

It implements the issuer API according to the spec but without any verification nor validation.

That means that it will return any certified credentials that a relying party requests.
