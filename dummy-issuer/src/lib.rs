use candid::{candid_method, Principal};
use canister_sig_util::signature_map::{SignatureMap, LABEL_SIG};
use canister_sig_util::CanisterSigPublicKey;
use ic_cdk::api::{set_certified_data, time};
use ic_cdk_macros::{query, update};
use ic_certification::{labeled_hash, Hash};
use identity_core::convert::FromJson;
use identity_credential::credential::Subject;
use identity_credential::error::Error as JwtVcError;
use identity_credential::validator::JwtValidationError;
use identity_jose::jws::Decoder;
use identity_jose::jwt::JwtClaims;
use lazy_static::lazy_static;
use serde_bytes::ByteBuf;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use vc_util::issuer_api::{
    ArgumentValue, CredentialSpec, DerivationOriginData, DerivationOriginError,
    DerivationOriginRequest, GetCredentialRequest, Icrc21ConsentInfo, Icrc21Error,
    Icrc21VcConsentMessageRequest, IssueCredentialError, IssuedCredentialData,
    PrepareCredentialRequest, PreparedCredentialData, SignedIdAlias,
};
use vc_util::{
    build_credential_jwt, did_for_principal, vc_jwt_to_jws, vc_signing_input,
    vc_signing_input_hash, CredentialParams,
};

const ISSUER_URL: &str = "https://dummy-issuer.vc";
const CREDENTIAL_URL_PREFIX: &str = "data:text/plain;charset=UTF-8,";
const MINUTE_NS: u64 = 60 * 1_000_000_000;
// The expiration of issued verifiable credentials.
const VC_EXPIRATION_PERIOD_NS: u64 = 15 * MINUTE_NS;

thread_local! {
    /// Non-stable structures
    // Canister signatures
    static SIGNATURES : RefCell<SignatureMap> = RefCell::new(SignatureMap::default());
}

lazy_static! {
    // Seed and public key used for signing the credentials.
    static ref CANISTER_SIG_SEED: Vec<u8> = hash_bytes("DummyIssuer").to_vec();
    static ref CANISTER_SIG_PK: CanisterSigPublicKey = CanisterSigPublicKey::new(ic_cdk::id(), CANISTER_SIG_SEED.clone());
}

fn hash_bytes(value: impl AsRef<[u8]>) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(value.as_ref());
    hasher.finalize().into()
}

fn update_root_hash() {
    SIGNATURES.with_borrow(|sigs| {
        set_certified_data(&labeled_hash(LABEL_SIG, &sigs.root_hash()));
    })
}

pub fn format_credential_spec(spec: &CredentialSpec) -> String {
    let mut description = format!("# Credential Type\n{}\n", spec.credential_type);

    if let Some(arguments) = &spec.arguments {
        description.push_str("## Arguments\n");
        for (key, value) in arguments {
            let value_str = match value {
                ArgumentValue::String(s) => s.clone(),
                ArgumentValue::Int(i) => i.to_string(),
            };
            description.push_str(&format!("- **{}**: {}\n", key, value_str));
        }
    } else {
        description.push_str("## Arguments\nNone\n");
    }

    description
}

#[update]
#[candid_method]
async fn vc_consent_message(
    req: Icrc21VcConsentMessageRequest,
) -> Result<Icrc21ConsentInfo, Icrc21Error> {
    Ok(Icrc21ConsentInfo {
        consent_message: format_credential_spec(&req.credential_spec),
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

fn jwt_error(custom_message: &'static str) -> JwtValidationError {
    JwtValidationError::CredentialStructure(JwtVcError::InconsistentCredentialJwtClaims(
        custom_message,
    ))
}

fn internal_error(msg: &str) -> IssueCredentialError {
    IssueCredentialError::Internal(String::from(msg))
}

// Decodes the id_alias JWS received from II and returns the principal.
// This function doesn't perform any verification of the signature.
fn get_id_alias(signed_id_alias: &SignedIdAlias) -> Result<Principal, JwtValidationError> {
    ///// Decode JWS.
    let decoder: Decoder = Decoder::new();
    let jws = decoder
        .decode_compact_serialization(&signed_id_alias.credential_jws.as_ref(), None)
        .map_err(|_| jwt_error("credential JWS parsing error"))?;
    let claims: JwtClaims<Value> = serde_json::from_slice(jws.claims())
        .map_err(|_| jwt_error("failed parsing JSON JWT claims"))?;
    let vc = claims
        .vc()
        .ok_or(jwt_error("missing \"vc\" claim in id_alias JWT claims"))?;
    let subject_value = vc.get("credentialSubject").ok_or(jwt_error(
        "missing \"credentialSubject\" claim in id_alias JWT vc",
    ))?;
    let subject = Subject::from_json_value(subject_value.clone())
        .map_err(|_| jwt_error("malformed \"credentialSubject\" claim in id_alias JWT vc"))?;
    let Value::Object(ref spec) = subject.properties["InternetIdentityIdAlias"] else {
        return Err(jwt_error(
            "missing \"InternetIdentityIdAlias\" claim in id_alias JWT vc",
        ));
    };
    let alias_value = spec.get("hasIdAlias").ok_or(jwt_error(
        "missing \"hasIdAlias\" parameter in id_alias JWT vc",
    ))?;
    let Value::String(alias) = alias_value else {
        return Err(jwt_error(
            "wrong type of \"hasIdAlias\" value in id_alias JWT vc",
        ));
    };
    let id_alias = Principal::from_text(alias)
        .map_err(|_| jwt_error("malformed \"hasIdAlias\"-value claim in id_alias JWT vc"))?;
    return Ok(id_alias);
}

fn exp_timestamp_s() -> u32 {
    ((time() + VC_EXPIRATION_PERIOD_NS) / 1_000_000_000) as u32
}

// Prepares a unique id for the given subject_principal.
// The returned URL has the format: "data:text/plain;charset=UTF-8,issuer:...,timestamp_ns:...,subject:..."
fn credential_id_for_principal(subject_principal: Principal) -> String {
    let issuer = format!("issuer:{}", ISSUER_URL);
    let timestamp = format!("timestamp_ns:{}", time());
    let subject = format!("subject:{}", subject_principal.to_text());
    format!(
        "{}{},{},{}",
        CREDENTIAL_URL_PREFIX, issuer, timestamp, subject
    )
}

fn verified_credential(subject_principal: Principal, credential_spec: &CredentialSpec) -> String {
    let params = CredentialParams {
        spec: credential_spec.clone(),
        subject_id: did_for_principal(subject_principal),
        credential_id_url: credential_id_for_principal(subject_principal),
        issuer_url: ISSUER_URL.to_string(),
        expiration_timestamp_s: exp_timestamp_s(),
    };
    build_credential_jwt(params)
}

#[update]
#[candid_method]
async fn prepare_credential(
    req: PrepareCredentialRequest,
) -> Result<PreparedCredentialData, IssueCredentialError> {
    let Ok(id_alias) = get_id_alias(&req.signed_id_alias) else {
        return Err(internal_error("Error getting id_alias"));
    };
    // let id_alias = get_id_alias(&req.signed_id_alias).unwrap_or(default);
    let credential_jwt = verified_credential(id_alias, &req.credential_spec);
    let signing_input =
        vc_signing_input(&credential_jwt, &CANISTER_SIG_PK).expect("failed getting signing_input");
    let msg_hash = vc_signing_input_hash(&signing_input);

    SIGNATURES.with(|sigs| {
        let mut sigs = sigs.borrow_mut();
        sigs.add_signature(&CANISTER_SIG_SEED, msg_hash);
    });
    update_root_hash();
    Ok(PreparedCredentialData {
        prepared_context: Some(ByteBuf::from(credential_jwt.as_bytes())),
    })
}

#[query]
#[candid_method(query)]
fn get_credential(req: GetCredentialRequest) -> Result<IssuedCredentialData, IssueCredentialError> {
    let prepared_context = match req.prepared_context {
        Some(context) => context,
        None => {
            return Result::<IssuedCredentialData, IssueCredentialError>::Err(internal_error(
                "missing prepared_context",
            ))
        }
    };
    let credential_jwt = match String::from_utf8(prepared_context.into_vec()) {
        Ok(s) => s,
        Err(_) => {
            return Result::<IssuedCredentialData, IssueCredentialError>::Err(internal_error(
                "invalid prepared_context",
            ))
        }
    };
    let signing_input =
        vc_signing_input(&credential_jwt, &CANISTER_SIG_PK).expect("failed getting signing_input");
    let message_hash = vc_signing_input_hash(&signing_input);
    let sig_result = SIGNATURES.with(|sigs| {
        let sig_map = sigs.borrow();
        sig_map.get_signature_as_cbor(&CANISTER_SIG_SEED, message_hash, None)
    });
    let sig = match sig_result {
        Ok(sig) => sig,
        Err(e) => {
            return Result::<IssuedCredentialData, IssueCredentialError>::Err(
                IssueCredentialError::SignatureNotFound(format!(
                    "signature not prepared or expired: {}",
                    e
                )),
            );
        }
    };
    let vc_jws =
        vc_jwt_to_jws(&credential_jwt, &CANISTER_SIG_PK, &sig).expect("failed constructing JWS");
    Result::<IssuedCredentialData, IssueCredentialError>::Ok(IssuedCredentialData { vc_jws })
}
