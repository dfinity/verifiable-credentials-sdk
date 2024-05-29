use candid::candid_method;
use ic_cdk_macros::{query, update};
use serde_bytes::ByteBuf;
use vc_util::issuer_api::{
    DerivationOriginData, DerivationOriginError, DerivationOriginRequest, GetCredentialRequest,
    Icrc21ConsentInfo, Icrc21Error, Icrc21VcConsentMessageRequest, IssueCredentialError,
    IssuedCredentialData, PrepareCredentialRequest, PreparedCredentialData,
};

#[update]
#[candid_method]
async fn vc_consent_message(
    req: Icrc21VcConsentMessageRequest,
) -> Result<Icrc21ConsentInfo, Icrc21Error> {
    Ok(Icrc21ConsentInfo {
        consent_message: format!(
            "Consent message from dummy issuer: {}",
            req.credential_spec.credential_type
        ),
        language: "en".to_string(),
    })
}

#[update]
#[candid_method]
async fn derivation_origin(
    req: DerivationOriginRequest,
) -> Result<DerivationOriginData, DerivationOriginError> {
    Ok(DerivationOriginData {
        origin: req.frontend_hostname,
    })
}

#[update]
#[candid_method]
async fn prepare_credential(
    _req: PrepareCredentialRequest,
) -> Result<PreparedCredentialData, IssueCredentialError> {
    Ok(PreparedCredentialData {
        prepared_context: Some(ByteBuf::new()),
    })
}

#[query]
#[candid_method(query)]
fn get_credential(
    _req: GetCredentialRequest,
) -> Result<IssuedCredentialData, IssueCredentialError> {
    Ok(IssuedCredentialData {
        vc_jws: "dummy-jwt".to_string(),
    })
}
