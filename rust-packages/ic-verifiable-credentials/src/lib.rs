use crate::issuer_api::CredentialSpec;
use base64::Engine;
use candid::Principal;
use ic_canister_sig_creation::{extract_raw_canister_sig_pk_from_der, CanisterSigPublicKey};
use ic_certification::Hash;
use ic_signature_verification::verify_canister_sig;
use identity_core::common::{Timestamp, Url};
use identity_core::convert::FromJson;
use identity_core::register_custom_now_utc;
use identity_credential::credential::{Credential, CredentialBuilder, Jwt, Subject};
use identity_credential::error::Error as JwtVcError;
use identity_credential::presentation::{
    JwtPresentationOptions, Presentation, PresentationBuilder,
};
use identity_credential::validator::JwtValidationError;
use identity_jose::jwk::{Jwk, JwkParams, JwkParamsOct, JwkType};
use identity_jose::jws::{
    CompactJwsEncoder, Decoder, JwsAlgorithm, JwsHeader, SignatureVerificationError,
    SignatureVerificationErrorKind,
};
use identity_jose::jwt::JwtClaims;
use identity_jose::jwu::{decode_b64, encode_b64};
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::ops::{Add, Deref, DerefMut};
use std::time::Duration;

pub mod issuer_api;

#[cfg(not(test))]
mod ic_time {
    use super::*;
    use ic_cdk::api::time;

    fn ic_now() -> Timestamp {
        Timestamp::from_unix(Duration::from_nanos(time()).as_secs() as i64).unwrap()
    }
    register_custom_now_utc!(ic_now);
}

pub const II_CREDENTIAL_URL_PREFIX: &str = "data:text/plain;charset=UTF-8,";
pub const II_ISSUER_URL: &str = "https://identity.ic0.app/";
pub const VC_SIGNING_INPUT_DOMAIN: &[u8; 26] = b"iccs_verifiable_credential";
pub const DID_ICP_PREFIX: &str = "did:icp:";
const II_MAINNET: &str = "rdmx6-jaaaa-aaaaa-aaadq-cai";

/// A pair of identities, that denote the same user.
/// Used in attribute sharing flow to maintain II's unlinkability of identities.
#[derive(Debug, Eq, PartialEq)]
pub struct AliasTuple {
    /// A temporary identity, used in attribute sharing flow.
    pub id_alias: Principal,
    /// An identity under which a user is known to a dapp.
    pub id_dapp: Principal,
    /// The derivation origin of the id_dapp
    pub derivation_origin: String,
}

#[derive(Debug, Eq, PartialEq)]
/// Parties that signed credentials contained in a verifiable presentation.
pub struct VcFlowSigners {
    pub ii_canister_id: Principal,
    pub ii_origin: String,
    pub issuer_canister_id: Principal,
    pub issuer_origin: String,
}

#[derive(Debug)]
pub enum CredentialVerificationError {
    InvalidJws(SignatureVerificationError),
    InvalidClaims(JwtValidationError),
}

#[derive(Debug)]
pub enum PresentationVerificationError {
    InvalidPresentationJwt(String),
    InvalidIdAliasCredential(CredentialVerificationError),
    InvalidRequestedCredential(CredentialVerificationError),
    Unknown(String),
}

/// Returns the effective bytes that will be signed when computing a canister signature for
/// the given JWT-credential, verifiable via the specified public key.
pub fn vc_signing_input(
    credential_jwt: &str,
    canister_sig_pk: &CanisterSigPublicKey,
) -> Result<Vec<u8>, String> {
    let encoder = jws_encoder(credential_jwt, canister_sig_pk)?;
    Ok(encoder.signing_input().to_vec())
}

/// Computes and returns SHA-256 hash of the given `signing_input` prefixed with
///      `length(VC_SIGNING_INPUT_DOMAIN) · VC_SIGNING_INPUT_DOMAIN`
/// (for domain separation), where `length(a)` is the length of byte-array `a`,
/// and `·` denotes concatenation of bytes.
pub fn vc_signing_input_hash(signing_input: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    let buf = [VC_SIGNING_INPUT_DOMAIN.len() as u8];
    hasher.update(buf);
    hasher.update(VC_SIGNING_INPUT_DOMAIN);
    hasher.update(signing_input);
    hasher.finalize().into()
}

/// Constructs and returns a JWS (a signed JWT) from the given components.
/// Specifically, it constructs a JWS-header with the given `canister_sig_pk`, and
/// packages `credential_jwt`, the header, and the signature `sig` into a JWS.
/// The given signature should be created over the bytes returned by `vc_signing_input()`.
/// Note: the validity of the signature is not checked.
pub fn vc_jwt_to_jws(
    credential_jwt: &str,
    canister_sig_pk: &CanisterSigPublicKey,
    sig: &[u8],
) -> Result<String, String> {
    let encoder = jws_encoder(credential_jwt, canister_sig_pk)?;
    Ok(encoder.into_jws(sig))
}

/// Constructs and returns a JWS (a signed JWT) from the given components.
/// The given `signing_input` should be a value returned by `vc_signing_input()`
/// (which already contains a header with canister signatures public key), and
/// `sig` should be valid canister signature over `signing_input`.
/// Note: the validity of the signature is not checked.
pub fn vc_signing_input_to_jws(signing_input: &[u8], sig: &[u8]) -> Result<String, String> {
    let decoder = Decoder::new();
    let bytes_with_separators = [signing_input, &[b'.']].concat();
    let parsed_signing_input = decoder
        .decode_compact_serialization(&bytes_with_separators, None)
        .unwrap();
    let header = parsed_signing_input
        .protected_header()
        .expect("internal: failed getting protected header");

    let encoder: CompactJwsEncoder = CompactJwsEncoder::new(parsed_signing_input.claims(), header)
        .map_err(|e| format!("internal: failed creating JWS encoder: {:?}", e))?;
    Ok(encoder.into_jws(sig))
}

/// Extracts the canister signature public key from the given signing_input, which is the
/// effective byte array that is signed when creating a JWS from a JWT.
/// (essentially, it is a serialized JWT with JWS header, yet without a signature,
/// cf. `vc_signing_input()`-function above).
pub fn canister_sig_pk_from_vc_signing_input(
    signing_input: &[u8],
) -> Result<CanisterSigPublicKey, String> {
    let decoder = Decoder::new();
    let bytes_with_separators = [signing_input, &[b'.']].concat();
    let parsed_signing_input = decoder
        .decode_compact_serialization(&bytes_with_separators, None)
        .map_err(|e| format!("internal: failed parsing signing_input: {:?}", e))?;
    let header = parsed_signing_input
        .protected_header()
        .expect("internal: failed getting protected header");
    let canister_sig_pk_raw = get_canister_sig_pk_raw(header)
        .map_err(|e| format!("internal: failed getting canister_sig_pk_raw: {:?}", e))?;
    CanisterSigPublicKey::try_from_raw(&canister_sig_pk_raw)
        .map_err(|e| format!("internal: failed parsing canister_sig_pk: {}", e))
}

/// Returns a DID for the given `principal`.
pub fn did_for_principal(principal: Principal) -> String {
    DID_ICP_PREFIX.to_string().add(&principal.to_string())
}

/// Returns a `principal` for the given DID.
pub fn principal_for_did(did: &str) -> Result<Principal, String> {
    if !did.starts_with(DID_ICP_PREFIX) {
        return Err(format!(
            "invalid DID: {}, expected prefix {}",
            did, DID_ICP_PREFIX
        ));
    }
    Principal::from_text(did.trim_start_matches(DID_ICP_PREFIX))
        .map_err(|e| format!("failed to parse DID: {}", e))
}

/// Verifies the given JWS-credential as an id_alias-VC and extracts the alias tuple.
/// Performs both the cryptographic verification of the credential, and the semantic
/// validation of the claims in the VC.
pub fn get_verified_id_alias_from_jws(
    credential_jws: &str,
    expected_vc_subject: &Principal,
    expected_derivation_origin: &str,
    signing_canister_id: &Principal,
    root_pk_raw: &[u8],
    current_time_ns: u128,
) -> Result<AliasTuple, CredentialVerificationError> {
    let claims = verify_credential_jws_with_canister_id(
        credential_jws,
        signing_canister_id,
        root_pk_raw,
        current_time_ns,
    )
    .map_err(CredentialVerificationError::InvalidJws)?;
    validate_claim("iss", II_ISSUER_URL, claims.iss())
        .map_err(CredentialVerificationError::InvalidClaims)?;
    let alias_tuple =
        extract_id_alias(&claims).map_err(CredentialVerificationError::InvalidClaims)?;
    if *expected_vc_subject != alias_tuple.id_dapp {
        return Err(CredentialVerificationError::InvalidClaims(
            inconsistent_jwt_claims("unexpected vc subject"),
        ));
    }

    if !matches_expected_origin(
        signing_canister_id,
        expected_derivation_origin,
        &alias_tuple,
    ) {
        return Err(CredentialVerificationError::InvalidClaims(
            inconsistent_jwt_claims("unexpected derivation origin"),
        ));
    }
    Ok(alias_tuple)
}

/// Verifies the specified JWS credential cryptographically and checks that the signature was
/// created by the provided canister.
/// DOES NOT perform semantic validation of the claims in the credential.
pub fn verify_credential_jws_with_canister_id(
    credential_jws: &str,
    signing_canister_id: &Principal,
    root_pk_raw: &[u8],
    current_time_ns: u128,
) -> Result<JwtClaims<Value>, SignatureVerificationError> {
    ///// Decode JWS.
    let decoder: Decoder = Decoder::new();
    let jws = decoder
        .decode_compact_serialization(credential_jws.as_ref(), None)
        .map_err(|e| invalid_signature_err(&format!("credential JWS parsing error: {}", e)))?;
    let signature = jws.decoded_signature();
    let message = signing_input_with_prefix(jws.signing_input());
    let jws_header = jws
        .protected_header()
        .ok_or(invalid_signature_err("missing JWS header"))?;
    let canister_sig_pk_der = get_canister_sig_pk_der(jws_header)?;
    let canister_sig_pk = CanisterSigPublicKey::try_from(canister_sig_pk_der.as_slice())
        .map_err(|e| key_decoding_err(&format!("invalid canister sig public key: {}", e)))?;

    if signing_canister_id != &canister_sig_pk.canister_id {
        return Err(invalid_signature_err(&format!(
            "canister sig canister id does not match provided canister id: expected {}, got {}",
            signing_canister_id.to_text(),
            canister_sig_pk.canister_id.to_text()
        )));
    }

    verify_canister_sig(
        &message,
        signature,
        canister_sig_pk_der.as_slice(),
        root_pk_raw,
    )
    .map_err(|e| invalid_signature_err(&format!("signature verification error: {}", e)))?;

    let claims: JwtClaims<Value> = serde_json::from_slice(jws.claims())
        .map_err(|e| invalid_signature_err(&format!("failed parsing JSON JWT claims: {}", e)))?;
    validate_expiration(claims.exp(), current_time_ns)
        .map_err(|e| invalid_signature_err(&format!("credential expired: {}", e)))?;
    Ok(claims)
}

fn extract_credentials_from_vp(vp_jwt: &str) -> Result<Vec<Jwt>, String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
    let payload = vp_jwt
        .split('.')
        .skip(1)
        .next()
        .ok_or("Failed to parse presentation JWT")?;
    let claims: Value = serde_json::from_slice(
        &BASE64
            .decode(payload)
            .map_err(|_| "Failed to decode base64")?,
    )
    .map_err(|_| "Failed to parse payload JSON")?;
    let credentials = claims
        .pointer("/vp/verifiableCredential")
        .ok_or("Failed to extract verifiableCredential claim")?
        .as_array()
        .ok_or("Invalid value for 'verifiableCredential' claim")?;
    Ok(credentials
        .into_iter()
        .flat_map(|value| value.as_str())
        .map(|jwt| Jwt::new(jwt.to_string()))
        .collect())
}

/// Verifies the specified JWT presentation cryptographically, which should contain exactly
/// two verifiable credentials (in the order specified):
///   1. An "Id alias" credential which links the effective subject of the VP to a temporary id_alias.
///      This credential should be signed by canister vc_flow_parties.ii_canister_id.
///   2. An actual credential requested by a user.  The subject of this credential is id_alias,
///      and it should be signed by canister vc_flow_parties.issuer_canister_id
/// Verifies that the subject of the first credential matches `effective_vc_subject`.
/// Returns the verified `effective_vc_subject` with id_alias, and the claims from the requested credential.
/// DOES NOT perform semantic validation of the returned claims.
pub fn verify_ii_presentation_jwt_with_canister_ids(
    vp_jwt: &str,
    effective_vc_subject: Principal,
    effective_derivation_origin: String,
    vc_flow_signers: &VcFlowSigners,
    root_pk_raw: &[u8],
    current_time_ns: u128,
) -> Result<(AliasTuple, JwtClaims<Value>), PresentationVerificationError> {
    let credentials = extract_credentials_from_vp(vp_jwt)
        .map_err(PresentationVerificationError::InvalidPresentationJwt)?;
    if credentials.len() != 2 {
        return Err(PresentationVerificationError::InvalidPresentationJwt(
            "expected exactly two verifiable credentials".to_string(),
        ));
    }
    let id_alias_vc_jws = credentials
        .first()
        .ok_or(PresentationVerificationError::Unknown(
            "missing id_alias vc".to_string(),
        ))?;
    let alias_tuple = get_verified_id_alias_from_jws(
        id_alias_vc_jws.as_str(),
        &effective_vc_subject,
        &effective_derivation_origin,
        &vc_flow_signers.ii_canister_id,
        root_pk_raw,
        current_time_ns,
    )
    .map_err(PresentationVerificationError::InvalidIdAliasCredential)?;
    let requested_vc_jws = credentials
        .get(1)
        .ok_or(PresentationVerificationError::Unknown(
            "missing requested vc".to_string(),
        ))?;
    let claims = verify_credential_jws_with_canister_id(
        requested_vc_jws.as_str(),
        &vc_flow_signers.issuer_canister_id,
        root_pk_raw,
        current_time_ns,
    )
    .map_err(|e| {
        PresentationVerificationError::InvalidRequestedCredential(
            CredentialVerificationError::InvalidJws(e),
        )
    })?;
    let requested_vc_subject = extract_subject(&claims).map_err(|e| {
        PresentationVerificationError::InvalidRequestedCredential(
            CredentialVerificationError::InvalidClaims(e),
        )
    })?;
    if requested_vc_subject != alias_tuple.id_alias {
        return Err(PresentationVerificationError::InvalidPresentationJwt(
            format!(
                "subject does not match id_alias: expected {}, got {}",
                alias_tuple.id_alias, requested_vc_subject
            )
            .to_string(),
        ));
    }
    Ok((alias_tuple, claims))
}

fn extract_vc_claims(claims: &JwtClaims<Value>) -> Result<Map<String, Value>, JwtValidationError> {
    let vc_claims = claims
        .custom()
        .ok_or(inconsistent_jwt_claims(
            "missing custom claims in JWT claims",
        ))?
        .as_object()
        .ok_or(inconsistent_jwt_claims(
            "malformed custom claims in JWT claims",
        ))?
        .get("vc")
        .ok_or(inconsistent_jwt_claims(
            "missing vc claims in JWT custom claims",
        ))?
        .as_object()
        .ok_or(inconsistent_jwt_claims(
            "malformed vc claims in JWT custom claims",
        ))?;
    Ok(vc_claims.clone())
}

/// Validates the provided presentation `vp_jwt`, both cryptographically and semantically:
///  - verifies the cryptographic consistency via `verify_ii_presentation_jwt_with_canister_ids(...)`.
///  - checks that the claims from the presentation match the credential spec `vc_spec`.
pub fn validate_ii_presentation_and_claims(
    vp_jwt: &str,
    effective_vc_subject: Principal,
    effective_derivation_origin: String,
    vc_flow_signers: &VcFlowSigners,
    vc_spec: &CredentialSpec,
    root_pk_raw: &[u8],
    current_time_ns: u128,
) -> Result<(), PresentationVerificationError> {
    let (_alias_tuple, claims) = verify_ii_presentation_jwt_with_canister_ids(
        vp_jwt,
        effective_vc_subject,
        effective_derivation_origin,
        vc_flow_signers,
        root_pk_raw,
        current_time_ns,
    )?;
    validate_claim("iss", &vc_flow_signers.issuer_origin, claims.iss())
        .map_err(invalid_requested_vc)?;
    let vc_claims = extract_vc_claims(&claims).map_err(invalid_requested_vc)?;
    validate_claims_match_spec(&vc_claims, vc_spec).map_err(invalid_requested_vc)?;
    Ok(())
}

pub struct CredentialParams {
    pub spec: CredentialSpec,
    pub subject_id: String,
    pub credential_id_url: String,
    pub issuer_url: String,
    pub expiration_timestamp_s: u32,
}

/// Builds a verifiable credential with the given parameters and returns the credential as a JWT-string.
pub fn build_credential_jwt(params: CredentialParams) -> String {
    let mut subject_json = json!({"id": params.subject_id});
    subject_json.as_object_mut().unwrap().insert(
        params.spec.credential_type.clone(),
        credential_spec_args_to_json(&params.spec),
    );
    let subject = Subject::from_json_value(subject_json).unwrap();
    let expiration_date = Timestamp::from_unix(params.expiration_timestamp_s as i64)
        .expect("internal: failed computing expiration timestamp");
    let credential: Credential = CredentialBuilder::default()
        .id(Url::parse(params.credential_id_url).unwrap())
        .issuer(Url::parse(params.issuer_url).unwrap())
        .type_(params.spec.credential_type)
        .subject(subject)
        .expiration_date(expiration_date)
        .build()
        .unwrap();
    credential.serialize_jwt(None).unwrap()
}

/// Builds from the given parameters a Verifiable Presentation as returned by II
/// to the relying party during a successful VC flow. Specifically, the returned JWT
///  * contains the two given VCs (`id_alias_vc_jws` and `requested_vc_jws`, in that order),
///  * contains the specified `holder`, which should match the subject of `id_alias_vc_jws`,
///  * does not contain a signature,
///
/// This function is not used by II directly (as the returned presentation is built by II-frontend),
/// but it is useful for testing RPs that should validate the presentations obtained from II.
///
/// NOTE: The given VCs are treated as opaque strings, and are NOT validated for syntax or contents,
/// i.e. the returned JWT can contain invalid information (if the parameters are invalid or inconsistent).
/// See also `verify_ii_presentation_jwt_with_canister_ids` for validation conditions.
pub fn build_ii_verifiable_presentation_jwt(
    holder: Principal,
    id_alias_vc_jws: String,
    requested_vc_jws: String,
) -> Result<String, String> {
    construct_verifiable_presentation_jwt(holder, vec![id_alias_vc_jws, requested_vc_jws])
}

fn credential_spec_args_to_json(spec: &CredentialSpec) -> serde_json::Value {
    let mut args_map = serde_json::Map::new();
    if let Some(args) = spec.arguments.as_ref() {
        for arg in args {
            args_map.insert(arg.0.clone(), arg.1.clone().into());
        }
    }
    serde_json::Value::Object(args_map)
}

/// Returns the given `signing_input` prefixed with
///      length(VC_SIGNING_INPUT_DOMAIN) || VC_SIGNING_INPUT_DOMAIN
/// (for domain separation).
fn signing_input_with_prefix(signing_input: &[u8]) -> Vec<u8> {
    let mut result = Vec::from([VC_SIGNING_INPUT_DOMAIN.len() as u8]);
    result.extend_from_slice(VC_SIGNING_INPUT_DOMAIN);
    result.extend_from_slice(signing_input);
    result
}

fn invalid_requested_vc(e: JwtValidationError) -> PresentationVerificationError {
    PresentationVerificationError::InvalidRequestedCredential(
        CredentialVerificationError::InvalidClaims(e),
    )
}
fn extract_subject(claims: &JwtClaims<Value>) -> Result<Principal, JwtValidationError> {
    let Some(sub) = claims.sub() else {
        return Err(JwtValidationError::CredentialStructure(
            JwtVcError::MissingSubject,
        ));
    };
    let subject = principal_for_did(sub)
        .map_err(|_| JwtValidationError::CredentialStructure(JwtVcError::InvalidSubject))?;
    Ok(subject)
}

fn extract_id_alias(claims: &JwtClaims<Value>) -> Result<AliasTuple, JwtValidationError> {
    let id_dapp = extract_subject(claims)?;
    let vc = extract_vc_claims(claims)?;
    let subject_value = vc.get("credentialSubject").ok_or(inconsistent_jwt_claims(
        "missing \"credentialSubject\" claim in id_alias JWT vc",
    ))?;
    let subject = Subject::from_json_value(subject_value.clone()).map_err(|_| {
        inconsistent_jwt_claims("malformed \"credentialSubject\" claim in id_alias JWT vc")
    })?;
    let Value::Object(ref spec) = subject.properties["InternetIdentityIdAlias"] else {
        return Err(inconsistent_jwt_claims(
            "missing \"InternetIdentityIdAlias\" claim in id_alias JWT vc",
        ));
    };
    let alias_value = spec.get("hasIdAlias").ok_or(inconsistent_jwt_claims(
        "missing \"hasIdAlias\" parameter in id_alias JWT vc",
    ))?;
    let Value::String(alias) = alias_value else {
        return Err(inconsistent_jwt_claims(
            "wrong type of \"hasIdAlias\" value in id_alias JWT vc",
        ));
    };
    let derivation_origin_value = spec.get("derivationOrigin").ok_or(inconsistent_jwt_claims(
        "missing \"derivationOrigin\" parameter in id_alias JWT vc",
    ))?;
    let Value::String(derivation_origin) = derivation_origin_value else {
        return Err(inconsistent_jwt_claims(
            "wrong type of \"derivationOrigin\" value in id_alias JWT vc",
        ));
    };
    let id_alias = Principal::from_text(alias).map_err(|_| {
        inconsistent_jwt_claims("malformed \"hasIdAlias\"-value claim in id_alias JWT vc")
    })?;
    Ok(AliasTuple {
        id_alias,
        id_dapp,
        derivation_origin: derivation_origin.clone(),
    })
}

fn validate_claim<T: PartialEq<S> + std::fmt::Display, S: std::fmt::Display>(
    label: &str,
    expected: T,
    actual: Option<S>,
) -> Result<(), JwtValidationError> {
    if let Some(actual) = actual {
        if expected == actual {
            Ok(())
        } else {
            println!(
                "inconsistent claim [{}] in VC::  expected: {}, actual: {}",
                label, expected, actual
            );
            Err(inconsistent_jwt_claims("inconsistent claim in VC"))
        }
    } else {
        println!("missing claim [{}] in VC", label);
        Err(inconsistent_jwt_claims("missing claim in VC"))
    }
}

fn validate_expiration(
    maybe_expiration_s: Option<i64>,
    current_time_ns: u128,
) -> Result<(), JwtValidationError> {
    if let Some(expiration_s) = maybe_expiration_s {
        let expiration_ns: u128 = (expiration_s * 1_000_000_000)
            .try_into()
            .map_err(|_| JwtValidationError::ExpirationDate)?;
        if expiration_ns > current_time_ns {
            Ok(())
        } else {
            Err(JwtValidationError::ExpirationDate)
        }
    } else {
        Err(JwtValidationError::CredentialStructure(
            JwtVcError::MissingExpirationDate,
        ))
    }
}

// Validates that provided `vc_claims` are consistent and match the given `spec`:
//  - `vc_claims` contain "type"-claim that contains `spec.credential_type`
//  - `vc_claims` contain claim named `spec.credential_type` with arguments that match `spec.arguments`,
//     cf. a convention at https://github.com/dfinity/internet-identity/blob/main/docs/vc-spec.md#recommended-convention-connecting-credential-specification-with-the-returned-credentials
pub fn validate_claims_match_spec(
    vc_claims: &Map<String, Value>,
    spec: &CredentialSpec,
) -> Result<(), JwtValidationError> {
    let credential_type = &spec.credential_type;

    // Check that type-claim contains spec.credential_type.
    let vc_type_entry = vc_claims
        .get("type")
        .ok_or(inconsistent_jwt_claims("missing type-claim"))?;
    let types = vc_type_entry
        .as_array()
        .ok_or(inconsistent_jwt_claims("malformed types-claim"))?;
    if !types.contains(&Value::String(credential_type.clone())) {
        return Err(inconsistent_jwt_claims(
            "missing credential_type in type-claim",
        ));
    };

    // Check that credentialSubject-claim contains spec.credential_type entry with matching arguments.
    let credential_subject = vc_claims
        .get("credentialSubject")
        .ok_or(inconsistent_jwt_claims("missing credentialSubject-claim"))?;
    let subject = Subject::from_json_value(credential_subject.clone())
        .map_err(|_| inconsistent_jwt_claims("malformed credentialSubject-claim"))?;
    let verified_claim_arguments = subject
        .properties
        .get(credential_type)
        .ok_or(inconsistent_jwt_claims("missing credential_type claim"))?
        .as_object()
        .ok_or(inconsistent_jwt_claims(
            "malformed credential_type arguments",
        ))?;
    let spec_arguments_count = spec.arguments.as_ref().map_or(0, |args| args.len());
    if spec_arguments_count != verified_claim_arguments.len() {
        return Err(inconsistent_jwt_claims(
            "wrong number of credential_type arguments",
        ));
    }
    if let Some(spec_arguments) = spec.arguments.as_ref() {
        for (key, value) in spec_arguments.iter() {
            if let Some(v) = verified_claim_arguments.get(key) {
                if value != v {
                    return Err(inconsistent_jwt_claims(
                        "wrong value in credential_type argument",
                    ));
                }
            } else {
                return Err(inconsistent_jwt_claims(
                    "missing key in credential_type arguments",
                ));
            }
        }
    }
    Ok(())
}

// Per https://datatracker.ietf.org/doc/html/rfc7518#section-6.4,
// JwkParamsOct are for symmetric keys or another key whose value is a single octet sequence.
fn canister_sig_pk_jwk(canister_sig_pk_der: &[u8]) -> Result<Jwk, String> {
    let mut cspk_jwk = Jwk::new(JwkType::Oct);
    cspk_jwk.set_alg("IcCs");
    cspk_jwk
        .set_params(JwkParams::Oct(JwkParamsOct {
            k: encode_b64(canister_sig_pk_der),
        }))
        .map_err(|e| format!("internal: failed creating JWK: {:?}", e))?;
    Ok(cspk_jwk)
}

fn jws_encoder<'a>(
    credential_jwt: &'a str,
    canister_sig_pk: &CanisterSigPublicKey,
) -> Result<CompactJwsEncoder<'a>, String> {
    let mut header: JwsHeader = JwsHeader::new();
    header.set_alg(JwsAlgorithm::Custom("IcCs".to_string()));
    let kid = did_for_principal(canister_sig_pk.canister_id);
    let jwk = canister_sig_pk_jwk(&canister_sig_pk.to_der())?;
    header.set_kid(kid);
    header.deref_mut().set_jwk(jwk);

    let encoder: CompactJwsEncoder = CompactJwsEncoder::new(credential_jwt.as_ref(), &header)
        .map_err(|e| format!("internal: failed creating JWS encoder: {:?}", e))?;
    Ok(encoder)
}

fn unsupported_alg_err(custom_message: &str) -> SignatureVerificationError {
    let err: SignatureVerificationError = SignatureVerificationErrorKind::UnsupportedAlg.into();
    err.with_custom_message(custom_message.to_string())
}

fn key_decoding_err(custom_message: &str) -> SignatureVerificationError {
    let err: SignatureVerificationError = SignatureVerificationErrorKind::KeyDecodingFailure.into();
    err.with_custom_message(custom_message.to_string())
}

fn invalid_signature_err(custom_message: &str) -> SignatureVerificationError {
    let err: SignatureVerificationError = SignatureVerificationErrorKind::InvalidSignature.into();
    err.with_custom_message(custom_message.to_string())
}

fn inconsistent_jwt_claims(custom_message: &'static str) -> JwtValidationError {
    JwtValidationError::CredentialStructure(JwtVcError::InconsistentCredentialJwtClaims(
        custom_message,
    ))
}

/// Extracts and returns the raw canister sig public key (without DER-prefix) from the given header.
pub fn get_canister_sig_pk_raw(
    jws_header: &JwsHeader,
) -> Result<Vec<u8>, SignatureVerificationError> {
    let pk_der = get_canister_sig_pk_der(jws_header)?;
    let pk_raw = extract_raw_canister_sig_pk_from_der(pk_der.as_slice())
        .map_err(|e| key_decoding_err(&e.to_string()))?;
    Ok(pk_raw)
}

/// Extracts and returns the DER encoded canister sig public key from the given header.
pub fn get_canister_sig_pk_der(
    jws_header: &JwsHeader,
) -> Result<Vec<u8>, SignatureVerificationError> {
    let jwk = jws_header
        .deref()
        .jwk()
        .ok_or(key_decoding_err("missing JWK in JWS header"))?;
    if jwk.alg() != Some("IcCs") {
        return Err(unsupported_alg_err("expected IcCs"));
    }
    // Per https://datatracker.ietf.org/doc/html/rfc7518#section-6.4,
    // JwkParamsOct are for symmetric keys or another key whose value is a single octet sequence.
    if jwk.kty() != JwkType::Oct {
        return Err(unsupported_alg_err("expected JWK of type oct"));
    }
    let jwk_params = jwk
        .try_oct_params()
        .map_err(|_| key_decoding_err("missing JWK oct params"))?;
    let pk_der = decode_b64(jwk_params.k.as_bytes())
        .map_err(|_| key_decoding_err("invalid base64url encoding"))?;
    Ok(pk_der)
}

fn construct_verifiable_presentation_jwt(
    holder: Principal,
    vcs_jws: Vec<String>,
) -> Result<String, String> {
    let holder_url = Url::parse(did_for_principal(holder)).map_err(|_| "Invalid holder")?;
    let mut builder = PresentationBuilder::new(holder_url, Default::default());
    for vc in vcs_jws {
        builder = builder.credential(Jwt::from(vc));
    }
    let presentation: Presentation<Jwt> = builder
        .build()
        .map_err(|_| "failed building presentation")?;
    presentation_to_compact_jwt(&presentation)
}

fn presentation_to_compact_jwt(presentation: &Presentation<Jwt>) -> Result<String, String> {
    let mut header: JwsHeader = JwsHeader::new();
    header.set_typ("JWT");
    header.set_alg(JwsAlgorithm::NONE);
    let vp_jwt = presentation
        .serialize_jwt(&JwtPresentationOptions {
            expiration_date: None,
            issuance_date: None,
            audience: None,
            custom_claims: None,
        })
        .map_err(|_| "failed serializing presentation")?;
    let encoder: CompactJwsEncoder = CompactJwsEncoder::new(vp_jwt.as_ref(), &header)
        .map_err(|_| "internal error: JWS encoder failed")?;
    Ok(encoder.into_jws(&[]))
}

// Returns the allowed domains based on the expected origin's domain and the signing canister.
// * If custom domain -> only use the custom domain.
// * If canister subdomain and II mainnet -> return the old domain only.
// * If canister subdomain and not II mainnet -> return the old and new domains.
fn allowed_origins(origin: &str, signing_canister_id: &Principal) -> Vec<String> {
    // Define the regex for matching canister domains of the format `https://<canister-id>[.raw].icp0`
    let origin_mapping_regex =
        Regex::new(r"^https://(?<subdomain>[\w-]+(?:\.raw)?)\.icp0\.io$").expect("Invalid regex");

    if let Some(captures) = origin_mapping_regex.captures(origin) {
        if let Some(subdomain) = captures.name("subdomain") {
            let legacy_domain = format!("https://{}.ic0.app", subdomain.as_str());
            if II_MAINNET == signing_canister_id.to_text() {
                return vec![legacy_domain];
            }
            return vec![legacy_domain, origin.to_string()];
        }
    }
    // Return the domain if it doesn't match a canister subdomain.
    vec![origin.to_string()]
}

// In order to give dapps a stable principal regardless whether they use the legacy (ic0.app) or the new domain (icp0.io)
// II maps back the derivation origin to the ic0.app domain.
// Therefore, we map expected origin given by the developer to the old domain.
// If the signing canister is II mainnet, we only allow the old domain to be the expected origin,
// because this is the current behavior of II mainnet.
fn matches_expected_origin(
    signing_canister_id: &Principal,
    expected_derivation_origin: &str,
    alias_tuple: &AliasTuple,
) -> bool {
    allowed_origins(expected_derivation_origin, signing_canister_id)
        .contains(&alias_tuple.derivation_origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issuer_api::ArgumentValue;
    use assert_matches::assert_matches;
    use ic_canister_sig_creation::{
        extract_raw_root_pk_from_der, IC_ROOT_PK_DER, IC_ROOT_PK_DER_PREFIX,
    };
    use std::collections::HashMap;

    const MINUTE_NS: u128 = 60 * 1_000_000_000;
    // Created in a local environment
    const TEST_IC_ROOT_PK_B64URL: &str = "MIGCMB0GDSsGAQQBgtx8BQMBAgEGDCsGAQQBgtx8BQMCAQNhAK32VjilMFayIiyRuyRXsCdLypUZilrL2t_n_XIXjwab3qjZnpR52Ah6Job8gb88SxH-J1Vw1IHxaY951Giv4OV6zB4pj4tpeY2nqJG77Blwk-xfR1kJkj1Iv-1oQ9vtHw";
    const ID_ALIAS_CREDENTIAL_JWS_NO_JWK: &str = "eyJraWQiOiJkaWQ6aWM6aWktY2FuaXN0ZXIiLCJhbGciOiJJY0NzIn0.eyJpc3MiOiJodHRwczovL2ludGVybmV0Y29tcHV0ZXIub3JnL2lzc3VlcnMvaW50ZXJuZXQtaWRlbml0eSIsIm5iZiI6MTYyMDMyODYzMCwianRpIjoiaHR0cHM6Ly9pbnRlcm5ldGNvbXB1dGVyLm9yZy9jcmVkZW50aWFsL2ludGVybmV0LWlkZW5pdHkiLCJzdWIiOiJkaWQ6d2ViOmNwZWhxLTU0aGVmLW9kamp0LWJvY2tsLTNsZHRnLWpxbGU0LXlzaTVyLTZiZmFoLXY2bHNhLXhwcmR2LXBxZSIsInZjIjp7IkBjb250ZXh0IjoiaHR0cHM6Ly93d3cudzMub3JnLzIwMTgvY3JlZGVudGlhbHMvdjEiLCJ0eXBlIjpbIlZlcmlmaWFibGVDcmVkZW50aWFsIiwiSW50ZXJuZXRJZGVudGl0eUlkQWxpYXMiXSwiY3JlZGVudGlhbFN1YmplY3QiOnsiaGFzX2lkX2FsaWFzIjoiZGlkOndlYjpzMzNxYy1jdG5wNS11Ynl6NC1rdWJxby1wMnRlbS1oZTRscy02ajIzai1od3diYS0zN3pibC10Mmx2My1wYWUifX19.2dn3omtjZXJ0aWZpY2F0ZVkBi9nZ96JkdHJlZYMBgwGDAYMCSGNhbmlzdGVygwJKAAAAAAAAAAABAYMBgwGDAYMCTmNlcnRpZmllZF9kYXRhggNYIG3uU_jutBtXB-of0uEA3RkCrcunK6D8QFPtX-gDSwDeggRYINLM_z_MXakw3sDoSiVB5lhRa0uxUB5w6LQQ5phqBX1gggRYIMULjwe1N6XomH10SEyc2r_uc7mGf1aSadeDaid9cUrkggRYIDw__VW2PgWMFp6mK-GmPG-7Fc90q58oK_wjcJ3IrkToggRYIAQTcQAtnxsa93zbfZEZV0f28OhiXL5Wp1OAyDHNI_x4ggRYINkQ8P9zGUvsVi3XbQ2bs6V_3kAiN8UNM6yPgeXfmArEgwGCBFggNVP2WB1Ts90nZG9hyLDaCww4gbhXxtw8R-poiMET62uDAkR0aW1lggNJgLiu1N2JpL4WaXNpZ25hdHVyZVgwqHrYoUsNvSEaSShbW8barx0_ODXD5ZBEl9nKOdkNy_fBmGErE_C7ILbC91_fyZ7CZHRyZWWDAYIEWCB223o-sI97tc3LwJL3LRxQ4If6v_IvfC1fwIGYYQ9vroMCQ3NpZ4MCWCA6UuW6rWVPRqQn_k-pP9kMNe6RKs1gj7QVCsaG4Bx2OYMBgwJYIHszMLDS2VadioIaHajRY5iJzroqMs63lVrs_Uj42j0sggNAggRYICm0w_XxGEw4fDPoYcojCILEi0qdH4-4Zw7klzdaPNOC";
    const TEST_CREDENTIAL_JWS_NO_EXPIRY: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQW9BQUFBQUFBQUFBUUVCeUk3dlEyOGVybHFnVjVMck03dTNIOUlaeGVwcUxzQkdnSjFyTldaX0tfQSJ9LCJraWQiOiJkaWQ6aWNwOnJya2FoLWZxYWFhLWFhYWFhLWFhYWFxLWNhaSIsImFsZyI6IkljQ3MifQ.eyJpc3MiOiJodHRwczovL2VtcGxveW1lbnQuaW5mby8iLCJuYmYiOjE2MjAzMjg2MzAsImp0aSI6Imh0dHBzOi8vZW1wbG95bWVudC5pbmZvL2NyZWRlbnRpYWxzLzQyIiwic3ViIjoiZGlkOmljcDp2aGJpYi1tNGhtNi1ocHZ5Yy03cHJkMi1zaWl2by1uYmQ3ci02N281eC1uM2F3aC1xc21xei13em5qZi10cWUiLCJ2YyI6eyJAY29udGV4dCI6Imh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIiwidHlwZSI6WyJWZXJpZmlhYmxlQ3JlZGVudGlhbCIsIlZlcmlmaWVkRW1wbG95ZWUiXSwiY3JlZGVudGlhbFN1YmplY3QiOnsiZW1wbG95ZWVfb2YiOnsiZW1wbG95ZXJJZCI6ImRpZDp3ZWI6ZGZpbml0eS5vcmciLCJlbXBsb3llck5hbWUiOiJERklOSVRZIEZvdW5kYXRpb24ifX19fQ.2dn3omtjZXJ0aWZpY2F0ZVkBsdnZ96JkdHJlZYMBgwGDAYMCSGNhbmlzdGVygwGCBFggq7DruGSK9j0nNpVYlgkE4OtYMHWfxzrqB0D-tTp77umDAkoAAAAAAAAAAQEBgwGDAYMBgwJOY2VydGlmaWVkX2RhdGGCA1ggc8y0K3LKbNnsixDTg2Ux51vwu6b9Kqm2NFykuHVtd06CBFgg0sz_P8xdqTDewOhKJUHmWFFrS7FQHnDotBDmmGoFfWCCBFggTwA0M58_LFASzZLk1ju6zhwQ6qzeDSZsYyc8Ak-WWGCCBFgg7bPsepWtwANz_eF2pBaMOy-a-UEVj8ojdMRGhxyIODqCBFggEflcBBzJzouB9GoAqyMJiiexVT1w7LIv72CbckA15-SCBFggFtwxSFgot33A2BgPFXCOTj9gM8Z0ORDn-YD1tYNW2wmDAYIEWCA1U_ZYHVOz3Sdkb2HIsNoLDDiBuFfG3DxH6miIwRPra4MCRHRpbWWCA0mAuK7U3YmkvhZpc2lnbmF0dXJlWDCisy0ljDwwuPOxJn72Y8qqxgxDRgP0srKPvFkEgygNfVHoEGnwseMBdMMrYzIStrNkdHJlZYMBggRYIAvQZNP5TRQHV7AavT2jNGPPLcQBzfQvva5hEybHvbw8gwJDc2lngwJYIHGZW4y0kE1oq6oGYkhXj36h1sNPmG2jwFX6tPGiRkfXgwJYICslyEcSADtGlWLKMBsBJAlXe8en4eGCuE9yuAnuqRBOggNA";
    const LOCAL_II_CANISTER_ID: &str = "rwlgt-iiaaa-aaaaa-aaaaa-cai";
    const LOCAL_SEED: [u8; 32] = [
        142, 84, 220, 222, 130, 185, 65, 67, 145, 152, 171, 78, 191, 101, 41, 107, 108, 94, 2, 122,
        56, 7, 17, 80, 17, 183, 249, 81, 212, 200, 233, 231,
    ];
    const TEST_CREDENTIAL_JWT: &str = r#"{"iss":"https://employment.info/","nbf":1620328630,"jti":"https://employment.info/credentials/42","sub":"did:icp:igfpm-3fhrp-syqme-4i4xk-o4pgd-5xdh4-fbbgw-jnxm5-bvou4-ljt52-kqe","vc":{"@context":"https://www.w3.org/2018/credentials/v1","type":["VerifiableCredential","VerifiedEmployee"],"credentialSubject":{"employee_of":{"employerId":"did:web:dfinity.org","employerName":"DFINITY Foundation"}}}}"#;
    const LOCAL_ISSUER_SIGNING_CANISTER_ID: &str = "rrkah-fqaaa-aaaaa-aaaaq-cai";

    // Created in a mainnet environment
    // ID_ALIAS credential pieces
    const ALIAS_JWS: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQW9BQUFBQUFHQUFKd0VCMWROcEZhMjNYSHVienc2YlA3MUVTb20yTFFwcUhmWjVhazdOWGRqeFdVQSJ9LCJraWQiOiJkaWQ6aWNwOmZndGU1LWNpYWFhLWFhYWFkLWFhYXRxLWNhaSIsImFsZyI6IkljQ3MifQ.eyJleHAiOjE3Mjk3NTgzNTksImlzcyI6Imh0dHBzOi8vaWRlbnRpdHkuaWMwLmFwcC8iLCJuYmYiOjE3Mjk3NTc0NTksImp0aSI6ImRhdGE6dGV4dC9wbGFpbjtjaGFyc2V0PVVURi04LHRpbWVzdGFtcF9uczoxNzI5NzU3NDU5MDQyODQ0NzkyLGFsaWFzX2hhc2g6ZDQ4OTIwM2EwOGExY2Q0N2YxY2QyOWVkZDEwODdhNTIxOWJhMDc2NzM4ZmUwYWU1YWUzOWNjZDlmMTFhNzUzMyIsInN1YiI6ImRpZDppY3A6N2Vib2ktdHl1eXMtYXFtNGMtdzJsN2ktdmd1Y20teHZhd3gtbGVtengtNmtxMmctZjUzdTcteXZmaDItbmFlIiwidmMiOnsiQGNvbnRleHQiOiJodHRwczovL3d3dy53My5vcmcvMjAxOC9jcmVkZW50aWFscy92MSIsInR5cGUiOlsiVmVyaWZpYWJsZUNyZWRlbnRpYWwiLCJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyJdLCJjcmVkZW50aWFsU3ViamVjdCI6eyJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyI6eyJoYXNJZEFsaWFzIjoiemo3ZmgtajNjZ3YtZW9qZHItaDU1aGEtNG82N3ItMzJ1dHUtMjJpdjItbzdoaHktM3lvb3QteGRjcmYtNXFlIiwiZGVyaXZhdGlvbk9yaWdpbiI6Imh0dHBzOi8vbDdydWEtcmFhYWEtYWFhYXAtYWhoNmEtY2FpLmljMC5hcHAifX19fQ.2dn3omtjZXJ0aWZpY2F0ZVkFbdnZ96NkdHJlZYMBgwGDAYIEWCCmWvo44iQiiGOonrtzz-Vc-cC15sj4dWw5iM14OXZei4MCSGNhbmlzdGVygwGDAYMBgwGDAYMBggRYIAIQTI4tvDRgL5ohfUDFZTkN99yxeUcFSZEPNIUMKZIVgwGCBFggQEL7KETbIG4XJKJI7vOT9csdIigPKY2Uj8GOCkCFM0ODAYIEWCCNPbxbGsgH608xO5FxLblP30pQBoIHcZ8cujd3GyrI74MCSgAAAAAAYAAnAQGDAYMBgwJOY2VydGlmaWVkX2RhdGGCA1ggK94e042ibWRcCDatgNq7FbfQvmk6k_gEnuLRHVktoaWCBFggbM1rsxpUdh1KVunP2MujhNW4-0cYToyhPLcOBPIgms6CBFggJxQXULs1dQvHYawdYMxqLUmGQD8B1du-ha8XhgEfBPSCBFggPeeB3ggR9ahGkWbFlPlDPZZvaG9PQGWtk5XjC_rBU-KCBFggyyqUBXAErjNvtSujkRfPkKqt7-At3-kgW8wTyPYVCgKCBFggvB-bTFT2brj8JTgekGQa5Z74fFkBhjVRYqUstIdSQsuCBFggb8yhZG3SMgMK91m93LVVhLWbI-8p3BVCgObtABGjCHeCBFgg9_rtUWU57ZbqOhjuhW_K5PePAOkqbSsD8qC8yNYyjP6CBFggJAf5RCyKZ6eeRpJCfSKCI65TMIwLHHJK8yOXVBNXs_KDAYIEWCDSNFYEA4p6QwQAd18Nc6Jp25bkaCmmZeMHfTuH_iQdnoMCRHRpbWWCA0n4gNPu6KfVgBhpc2lnbmF0dXJlWDCS57wG5PXgM1EPsxX76Z9YUWSD98WNfz65NnZBZoXGmyBFazFCFS9l7dT2602nOy5qZGVsZWdhdGlvbqJpc3VibmV0X2lkWB0sVbNH7PJobIN4HWxZ0bQ-e0y6jetsGzdhB_LNAmtjZXJ0aWZpY2F0ZVkClNnZ96JkdHJlZYMBggRYIMn7ubxmVJ-rn7nj6UC7uIZSpjd9DZIZQkBUqkfLmjUggwGDAYIEWCBYlr0e6vPqPOEgzNI1ULovxaF5f0mqIr0kZk0gGA3mj4MCRnN1Ym5ldIMBgwGDAYMBggRYIIc5-77dPe2qj-9BhwNnwJBb3jdrY9034rF2-wi1ggUvgwGCBFggg_I0Z_VeOyicnsbKOf6cAu9lvR5u1cEYZsKqvna-qnaDAYMCWB0sVbNH7PJobIN4HWxZ0bQ-e0y6jetsGzdhB_LNAoMBgwJPY2FuaXN0ZXJfcmFuZ2VzggNYMtnZ94KCSgAAAAAAYAAAAQFKAAAAAABgAK4BAYJKAAAAAABgALABAUoAAAAAAG___wEBgwJKcHVibGljX2tleYIDWIUwgYIwHQYNKwYBBAGC3HwFAwECAQYMKwYBBAGC3HwFAwIBA2EAkAdRIHeOshpTCgK8x2Pn9KGSkzUGlmr3tUwQpNKyTeaoayAONEC65iZ79MSI2aEdBHLDjBtiIRmPmOTmiCujilpOOqWvzombf4Je2VrfoSYpaIBzVW8nR1JyE-jXPkDOggRYIDbzzSV9kPs45CWX8ZOl4DHb1YW2KSeTuwTbR5SAPOBuggRYIIj-oNtp84-c8_uoj4oEDzytya53cvoaQGpupGT6hYueggRYIGlh7xN8Ku4LBGcILvbTwSwD6TATtgKky2IUJw5ISGPxggRYIH4XYSU8iq7e4e1D2Iu1vaYwwPH4nnQKa88RkZ8eDR8LgwJEdGltZYIDScO7z9Hp8ZuAGGlzaWduYXR1cmVYMJSfsuzf0IsuGWBkZnOkzxXFhJHg30mRgqLNMxQLgcKrygFQuYg2iR4BecwAja35ZmR0cmVlgwGCBFggY_VDRlzjs-wNRdGdOHAK2Qfbt6qCiMuZ0VxLk2vfbziDAkNzaWeDAYIEWCAMbgXnP8FSPBVYUcL1bvbtHEYgLUFzvvhw-QR-UW6Uv4MCWCDz48dIHnkz9PyXWqYC1lBlRQSCFT7a2tJpWgMWbvMOoIMBgwJYID1jLiMLV5LdNLLULW3ax_err0pz6iJJUhe8uBMAiqzdggNAggRYIOIQ-FDH9cCtWre7kxNlvAKzOyNp6vymkVPgsJzxl6ve";
    const ALIAS_ID_PRINCIPAL: &str =
        "zj7fh-j3cgv-eojdr-h55ha-4o67r-32utu-22iv2-o7hhy-3yoot-xdcrf-5qe";
    const ALIAS_DAPP_PRINCIPAL: &str =
        "7eboi-tyuys-aqm4c-w2l7i-vgucm-xvawx-lemzx-6kq2g-f53u7-yvfh2-nae";
    const ALIAS_EXPIRY_NS: u128 = 1729758359 * 1_000_000_000; // from ID_ALIAS_CREDENTIAL_JWS
    const ALIAS_CURRENT_TIME_AFTER_EXPIRY_NS: u128 = ALIAS_EXPIRY_NS + MINUTE_NS;
    const ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS: u128 = ALIAS_EXPIRY_NS - MINUTE_NS;
    // Verifiable Presentation pieces
    const ISSUER_URL: &str = "https://dummy-issuer.vc/";
    // The origin in the credentials uses the old domain for historical reasons.
    const RP_DERIVATION_ORIGIN_OLD_DOMAIN: &str = "https://l7rua-raaaa-aaaap-ahh6a-cai.ic0.app";
    // Yet, the credential is still valid if he developers check with the new domain.
    const RP_DERIVATION_ORIGIN_NEW_DOMAIN: &str = "https://l7rua-raaaa-aaaap-ahh6a-cai.icp0.io";
    const VP_ID_ALIAS: &str = "7irwo-r5t2f-454sx-mkymz-ewrsg-o6oba-ol5jw-2wpns-yoxpi-5uego-vqe";
    const VP_RP_ID: &str = "7eboi-tyuys-aqm4c-w2l7i-vgucm-xvawx-lemzx-6kq2g-f53u7-yvfh2-nae";
    const VP_ID_ALIAS_JWS: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQW9BQUFBQUFHQUFKd0VCXzFBQ2lleTUwd0VkZERTbUkwcU9WLXRZR1JPaHo1TFByMnR1em4wSmJPayJ9LCJraWQiOiJkaWQ6aWNwOmZndGU1LWNpYWFhLWFhYWFkLWFhYXRxLWNhaSIsImFsZyI6IkljQ3MifQ.eyJleHAiOjE3Mjk3NTg0MTcsImlzcyI6Imh0dHBzOi8vaWRlbnRpdHkuaWMwLmFwcC8iLCJuYmYiOjE3Mjk3NTc1MTcsImp0aSI6ImRhdGE6dGV4dC9wbGFpbjtjaGFyc2V0PVVURi04LHRpbWVzdGFtcF9uczoxNzI5NzU3NTE3MjYyMDYyMzA1LGFsaWFzX2hhc2g6ZGY4ZjkwOTk0NGQ1MjhhMWQ2ODYwOTFiZTM5YWQwNzUyMjEzYWJhMWQ0MDY2ZWJjZDg3ZDNlNmMzYmVkOTlkZCIsInN1YiI6ImRpZDppY3A6N2Vib2ktdHl1eXMtYXFtNGMtdzJsN2ktdmd1Y20teHZhd3gtbGVtengtNmtxMmctZjUzdTcteXZmaDItbmFlIiwidmMiOnsiQGNvbnRleHQiOiJodHRwczovL3d3dy53My5vcmcvMjAxOC9jcmVkZW50aWFscy92MSIsInR5cGUiOlsiVmVyaWZpYWJsZUNyZWRlbnRpYWwiLCJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyJdLCJjcmVkZW50aWFsU3ViamVjdCI6eyJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyI6eyJkZXJpdmF0aW9uT3JpZ2luIjoiaHR0cHM6Ly9sN3J1YS1yYWFhYS1hYWFhcC1haGg2YS1jYWkuaWMwLmFwcCIsImhhc0lkQWxpYXMiOiI3aXJ3by1yNXQyZi00NTRzeC1ta3ltei1ld3JzZy1vNm9iYS1vbDVqdy0yd3Bucy15b3hwaS01dWVnby12cWUifX19fQ.2dn3omtjZXJ0aWZpY2F0ZVkFbdnZ96NkdHJlZYMBgwGDAYIEWCCmWvo44iQiiGOonrtzz-Vc-cC15sj4dWw5iM14OXZei4MCSGNhbmlzdGVygwGDAYMBgwGDAYMBggRYIAIQTI4tvDRgL5ohfUDFZTkN99yxeUcFSZEPNIUMKZIVgwGCBFggQEL7KETbIG4XJKJI7vOT9csdIigPKY2Uj8GOCkCFM0ODAYIEWCCNPbxbGsgH608xO5FxLblP30pQBoIHcZ8cujd3GyrI74MCSgAAAAAAYAAnAQGDAYMBgwJOY2VydGlmaWVkX2RhdGGCA1gg-MJWWQNUavYDofzwqvq-zNimXmetUcqB-w0LrNC_sYeCBFggbM1rsxpUdh1KVunP2MujhNW4-0cYToyhPLcOBPIgms6CBFggJxQXULs1dQvHYawdYMxqLUmGQD8B1du-ha8XhgEfBPSCBFggPeeB3ggR9ahGkWbFlPlDPZZvaG9PQGWtk5XjC_rBU-KCBFggyyqUBXAErjNvtSujkRfPkKqt7-At3-kgW8wTyPYVCgKCBFggvB-bTFT2brj8JTgekGQa5Z74fFkBhjVRYqUstIdSQsuCBFggb8yhZG3SMgMK91m93LVVhLWbI-8p3BVCgObtABGjCHeCBFgg9_rtUWU57ZbqOhjuhW_K5PePAOkqbSsD8qC8yNYyjP6CBFgg8py_vRzM9Bm27F6IyXzWJiXqXDYZ8jfITu7XsaCfLl2DAYIEWCBXFcGPEL9l1azWEHFo-ZfuWXq0Pfy08LD-ps3EjLmQ-YMCRHRpbWWCA0mnt6Cnw6nVgBhpc2lnbmF0dXJlWDCR1BHhBERls_WaxgULheuhLoNs5bbFDw7QiyiIGuxRaUBlRfAp8u91seVbpVkoJYNqZGVsZWdhdGlvbqJpc3VibmV0X2lkWB0sVbNH7PJobIN4HWxZ0bQ-e0y6jetsGzdhB_LNAmtjZXJ0aWZpY2F0ZVkClNnZ96JkdHJlZYMBggRYIMn7ubxmVJ-rn7nj6UC7uIZSpjd9DZIZQkBUqkfLmjUggwGDAYIEWCBYlr0e6vPqPOEgzNI1ULovxaF5f0mqIr0kZk0gGA3mj4MCRnN1Ym5ldIMBgwGDAYMBggRYIIc5-77dPe2qj-9BhwNnwJBb3jdrY9034rF2-wi1ggUvgwGCBFggg_I0Z_VeOyicnsbKOf6cAu9lvR5u1cEYZsKqvna-qnaDAYMCWB0sVbNH7PJobIN4HWxZ0bQ-e0y6jetsGzdhB_LNAoMBgwJPY2FuaXN0ZXJfcmFuZ2VzggNYMtnZ94KCSgAAAAAAYAAAAQFKAAAAAABgAK4BAYJKAAAAAABgALABAUoAAAAAAG___wEBgwJKcHVibGljX2tleYIDWIUwgYIwHQYNKwYBBAGC3HwFAwECAQYMKwYBBAGC3HwFAwIBA2EAkAdRIHeOshpTCgK8x2Pn9KGSkzUGlmr3tUwQpNKyTeaoayAONEC65iZ79MSI2aEdBHLDjBtiIRmPmOTmiCujilpOOqWvzombf4Je2VrfoSYpaIBzVW8nR1JyE-jXPkDOggRYIDbzzSV9kPs45CWX8ZOl4DHb1YW2KSeTuwTbR5SAPOBuggRYIIj-oNtp84-c8_uoj4oEDzytya53cvoaQGpupGT6hYueggRYIGlh7xN8Ku4LBGcILvbTwSwD6TATtgKky2IUJw5ISGPxggRYIH4XYSU8iq7e4e1D2Iu1vaYwwPH4nnQKa88RkZ8eDR8LgwJEdGltZYIDScO7z9Hp8ZuAGGlzaWduYXR1cmVYMJSfsuzf0IsuGWBkZnOkzxXFhJHg30mRgqLNMxQLgcKrygFQuYg2iR4BecwAja35ZmR0cmVlgwGCBFggY_VDRlzjs-wNRdGdOHAK2Qfbt6qCiMuZ0VxLk2vfbziDAkNzaWeDAYIEWCA9UHBJbHOr-E1JQlly9iFGQCsTYLyd9lD3QCqcqJRFnYMBgwJYINuSM7LpfR2lCwBy0b3st88K7FYqd13Q3uK_YsH5ggu7gwGDAlggPolR0pCAtERu83fvIuzIlLuhfQ8f5M5AIY492yiyQFmCA0CCBFggeLEHcUhq_M_QAshlwqu4hNd_W7ubAKyWtRM2bcorZ-KCBFggZVFrcgHpS48RZ-Hq0IuPRUSEcVeZzcHNNAQ0Xm4QZu8";
    const VP_VC_JWS: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQW9BQUFBQUFlQTZRUUVCbm9vVTJ5Mmg5OGRtLXFBcnJsX1BVUXRERWJpMURCOVRNUHNscGlfZ2VsZyJ9LCJraWQiOiJkaWQ6aWNwOnFkaWlmLTJpYWFhLWFhYWFwLWFoamFxLWNhaSIsImFsZyI6IkljQ3MifQ.eyJleHAiOjE3Mjk3NTg0MjEsImlzcyI6Imh0dHBzOi8vZHVtbXktaXNzdWVyLnZjLyIsIm5iZiI6MTcyOTc1NzUyMSwianRpIjoiZGF0YTp0ZXh0L3BsYWluO2NoYXJzZXQ9VVRGLTgsaXNzdWVyOmh0dHBzOi8vZHVtbXktaXNzdWVyLnZjLHRpbWVzdGFtcF9uczoxNzI5NzU3NTIxNTYxMjQ5NDYxLHN1YmplY3Q6N2lyd28tcjV0MmYtNDU0c3gtbWt5bXotZXdyc2ctbzZvYmEtb2w1anctMndwbnMteW94cGktNXVlZ28tdnFlIiwic3ViIjoiZGlkOmljcDo3aXJ3by1yNXQyZi00NTRzeC1ta3ltei1ld3JzZy1vNm9iYS1vbDVqdy0yd3Bucy15b3hwaS01dWVnby12cWUiLCJ2YyI6eyJAY29udGV4dCI6Imh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIiwidHlwZSI6WyJWZXJpZmlhYmxlQ3JlZGVudGlhbCIsIlRlc3QiXSwiY3JlZGVudGlhbFN1YmplY3QiOnsiVGVzdCI6eyJvbmUiOiJhcmcifX19fQ.2dn3omtjZXJ0aWZpY2F0ZVkGYNnZ96NkdHJlZYMBgwGDAYIEWCCmWvo44iQiiGOonrtzz-Vc-cC15sj4dWw5iM14OXZei4MCSGNhbmlzdGVygwGDAYIEWCB7iIkd6OMv3l8FvDYrPPRf40oKIINuEOMMCh1dNiNsoYMBggRYIFZdHl_2XSlbf_gK9xI4plYBg_HNvsuIcj0aC6U24yKKgwGCBFggqh1AxVnNJYrl9iubozBfBx4at5nZ1zLVW9-eQ5uKNRqDAYMBgwGCBFggzXSra1qZIgamCo1xqb1tQwNv_sBsdRNT5fXvXXG2w2qDAYIEWCCJLep1ardYpzTYQ0WZ8GlQ9GHwROEAhgz2A6fvpieny4MBgwGDAYIEWCDqG7W3yWGUsBUqwVbTDLwI4AQZ2jd0hKqRoGWIGj4SS4MBggRYIKf-fCBA3z7O7cree82myYt4L79UgWaLeFbDQGAzVmpWgwGCBFggfQ158wn1kzlfglkz8C7EnB0LecwVg19ll66tbyyUaSWDAYMBgwJKAAAAAAHgOkEBAYMBgwGDAk5jZXJ0aWZpZWRfZGF0YYIDWCAPzGwYY7CuRUBmO7vHYhfwL3-3sfPDbKG0t315wCvlN4IEWCCuSzlohUja0dr-O5mt3TV1TQ9FWXLlWIUZqsQEOgFfEYIEWCAJbqUoiDNS0mnzLg688iSz83zRYO4Bbqs-81lxOEXd_4IEWCD-ibIPzR0HeXg53bWZTaev-U5Ji9a2uxix3uw-DOUoboIEWCA-mUrRA565PinuauC9TMsxvi1I4Sk3dcGlw-qVWHTYY4IEWCB3ZkdTLr39xQzfdi45JKorZxgMBHMTBa5ykTqWU9mUMIIEWCC7fBbv0A8-HnKoWK8_xOcOvY4qVaMyLVaSvwewO6dShYIEWCByF4_R1jGX-1yIZ3Ejk5gT7fJbuK-hqXUMikRsfsdiWoIEWCAVCa22l1lP6Bzi_YZ8_VR_wrY61VjgVko6FQraSKXHyIIEWCAczYwBD93TEZOzGOnu6xBfWY3dNVIkxjiK3F_b6gF6T4IEWCCuw-CNxZLiXJ7dvBm-mYkslLR0-yRJNZalq7cQVVwvJ4MBggRYIAAFKTZc0hfDrM93AqJN5wyH-Lrsj_gpx0U0U5nenE_3gwJEdGltZYIDSf2r_8HTqdWAGGlzaWduYXR1cmVYMKKEmaPn7TIrYslnLKo8OTX3yzCF3Krb6EZS37BcwEvcTIxZM35Ev-EjHTMAKYeqqmpkZWxlZ2F0aW9uomlzdWJuZXRfaWRYHZNlqMnyf0EwYIUINdGFmkFcZ1dc0wwEoYP9CWMCa2NlcnRpZmljYXRlWQJ92dn3omR0cmVlgwGCBFggcRdzGWZnTQ5A8-lF0d5DPi3_5f7PJ0KZwD3Xv-NNczqDAYMBggRYIGu6ttvuGMo7I1jUvI7-B3GVagaeqLQRiLIvL0_wloEfgwJGc3VibmV0gwGDAYIEWCC4S7dwYkrBdIen5iHdcRbw-AzYvBMkC9Y6yPRmP1_yUIMBgwGCBFggZU8iKg13hclAa0zDCXDq5KxOmSGRqiCXAFoxWUis6duDAYIEWCDICaL3HEg2eeoIP8o9pYN94pBt4HMVnHZrQ5Y-nu7qaIMBggRYIJbo23yaKDJu6we2Bfu4YIRYtg6mBWqOF-MXxgX0gOwpgwJYHZNlqMnyf0EwYIUINdGFmkFcZ1dc0wwEoYP9CWMCgwGDAk9jYW5pc3Rlcl9yYW5nZXOCA1gb2dn3gYJKAAAAAAHgAAABAUoAAAAAAe___wEBgwJKcHVibGljX2tleYIDWIUwgYIwHQYNKwYBBAGC3HwFAwECAQYMKwYBBAGC3HwFAwIBA2EAipGyOwmK633V5SVxfwwp05Z8X05h0LXHxpcXLm9WgqpeQj1CB3aVxRerp-O2NtqUCTjX1cru4mw8ccxoiPVCxZZuYOpNkd25WtCZQoMXEJfjJQJr7wAxKLRGQQVshtBKggRYINlpKzO5NiHfpbZBSucSrQiCoPYpdsN8UOrcpV72ZomQggRYIGiYSAtwdM46Hcl3Q-_Sfb9-3zl6QUzT9Ai5imuRmo2ygwJEdGltZYIDSbi11Jqx6Kj_F2lzaWduYXR1cmVYMIcgbcKXr2zVU1TnnuZkxIUamscPJPxElbp5sYgzDB0DSWufUiI9zOchzIMBYJ80qWR0cmVlgwJDc2lngwJYIGuaQaib12ag5Lo5gB8FKLYAthE567jSV7PzWLEvIAsNgwGCBFgg_4A4YAApueE_Okpwdk3fQTVbz0m1lBPdcGcAjBVlA6iDAlggtYRNmXNW29rWs_4hiI9dg4sHAjLRJIjaD0lGloBYyEKCA0A";
    const VP_EXPIRY_NS: u128 = 1729758417 * 1_000_000_000; // from ID_ALIAS_CREDENTIAL_JWS
    const VP_CURRENT_TIME_AFTER_EXPIRY_NS: u128 = VP_EXPIRY_NS + MINUTE_NS;
    const VP_CURRENT_TIME_BEFORE_EXPIRY_NS: u128 = VP_EXPIRY_NS - MINUTE_NS;

    fn test_time() -> Timestamp {
        Timestamp::from_unix(
            Duration::from_nanos(ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS as u64).as_secs() as i64,
        )
        .unwrap()
    }
    register_custom_now_utc!(test_time);

    fn local_ic_root_pk_raw() -> Vec<u8> {
        let pk_der = decode_b64(TEST_IC_ROOT_PK_B64URL).expect("failure decoding canister pk");
        extract_raw_root_pk_from_der(pk_der.as_slice())
            .expect("failure extracting local root pk from DER")
    }

    fn mainnet_ic_root_pk_raw() -> Vec<u8> {
        extract_raw_root_pk_from_der(IC_ROOT_PK_DER.as_slice())
            .expect("failure extracting mainnet root pk from DER")
    }

    fn local_ii_canister_sig_pk() -> CanisterSigPublicKey {
        CanisterSigPublicKey::new(
            Principal::from_text(LOCAL_II_CANISTER_ID).expect("wrong principal"),
            LOCAL_SEED.to_vec(),
        )
    }

    fn local_issuer_canister_sig_pk() -> CanisterSigPublicKey {
        CanisterSigPublicKey::new(
            Principal::from_text(LOCAL_ISSUER_SIGNING_CANISTER_ID).expect("wrong principal"),
            LOCAL_SEED.to_vec(),
        )
    }

    fn mainnet_ii_canister_sig_pk() -> CanisterSigPublicKey {
        let pk_der = decode_b64(
            "MDwwDAYKKwYBBAGDuEMBAgMsAAoAAAAAAGAAJwEB_1ACiey50wEddDSmI0qOV-tYGROhz5LPr2tuzn0JbOk",
        )
        .expect("failure decoding ii canister pk");
        CanisterSigPublicKey::try_from(pk_der.as_slice()).expect("wrong pk for ii in mainnet")
    }

    fn mainnet_issuer_canister_sig_pk() -> CanisterSigPublicKey {
        let pk_der = decode_b64(
            "MDwwDAYKKwYBBAGDuEMBAgMsAAoAAAAAAeA6QQEBnooU2y2h98dm-qArrl_PUQtDEbi1DB9TMPslpi_gelg",
        )
        .expect("failure decoding issuer canister pk");
        CanisterSigPublicKey::try_from(pk_der.as_slice()).expect("Wrong pk for issuer in mainnet")
    }

    fn alias_principal() -> Principal {
        Principal::from_text(ALIAS_ID_PRINCIPAL).expect("wrong id alias principal")
    }

    fn dapp_principal() -> Principal {
        Principal::from_text(ALIAS_DAPP_PRINCIPAL).expect("wrong dapp principal")
    }

    fn claims_from_jws(credential_jws: &str) -> JwtClaims<Value> {
        let decoder: Decoder = Decoder::new();
        let jws = decoder
            .decode_compact_serialization(credential_jws.as_ref(), None)
            .expect("failed JWS parsing");
        let claims: JwtClaims<Value> =
            serde_json::from_slice(jws.claims()).expect("failed parsing JSON JWT claims");
        claims
    }

    fn mainnet_test_vc_flow_signers() -> VcFlowSigners {
        VcFlowSigners {
            ii_canister_id: mainnet_ii_canister_sig_pk().canister_id,
            ii_origin: II_ISSUER_URL.to_string(),
            issuer_canister_id: mainnet_issuer_canister_sig_pk().canister_id,
            issuer_origin: ISSUER_URL.to_string(),
        }
    }

    #[test]
    fn should_compute_domain_separated_signing_input_hash() {
        let signing_input = b"some bytes to sign";
        let signing_input_with_prefix = signing_input_with_prefix(signing_input.as_slice());
        assert_eq!(26, signing_input_with_prefix[0]);
        assert_eq!(
            b"iccs_verifiable_credential".as_slice(),
            signing_input_with_prefix[1..27].to_vec().as_slice()
        );
        let util_hash = vc_signing_input_hash(signing_input);
        let mut hasher = Sha256::new();
        hasher.update(signing_input_with_prefix);
        let manual_hash: Hash = hasher.finalize().into();
        assert_eq!(util_hash, manual_hash);
    }

    #[test]
    fn should_construct_correct_jws() {
        let canister_id = Principal::from_text(LOCAL_II_CANISTER_ID).expect("wrong principal");
        let canister_sig_pk = CanisterSigPublicKey::new(canister_id, LOCAL_SEED.to_vec());
        let dummy_sig: &str = "some signature";
        let credential_jwt = String::from_utf8(TEST_CREDENTIAL_JWT.into()).expect("wrong JWT");
        let credential_jws = vc_jwt_to_jws(&credential_jwt, &canister_sig_pk, dummy_sig.as_bytes())
            .expect("failed constructing JWS");
        let signing_input = vc_signing_input(&credential_jwt, &canister_sig_pk)
            .expect("failed constructing signing input");
        let credential_jws_from_signing_input =
            vc_signing_input_to_jws(signing_input.as_slice(), dummy_sig.as_bytes())
                .expect("failed constructing JWS");
        assert_eq!(credential_jws_from_signing_input, credential_jws);

        let decoder: Decoder = Decoder::new();
        let jws = decoder
            .decode_compact_serialization(credential_jws.as_ref(), None)
            .expect("Failed parsing constructed JWS");
        assert_eq!(dummy_sig.as_bytes(), jws.decoded_signature());
        let jws_header = jws.protected_header().expect("JWS without header");
        let canister_sig_pk_from_jws =
            get_canister_sig_pk_raw(jws_header).expect("JWS header without pk");
        let canister_sig_pk_raw =
            extract_raw_canister_sig_pk_from_der(canister_sig_pk.to_der().as_slice())
                .expect("wrong canister sig pk");
        assert_eq!(canister_sig_pk_from_jws, canister_sig_pk_raw);
        assert_eq!(jws.claims(), TEST_CREDENTIAL_JWT.as_bytes());
    }

    #[test]
    fn should_extract_canister_sig_pk_from_signing_input() {
        let canister_id = Principal::from_text(LOCAL_II_CANISTER_ID).expect("wrong principal");
        let canister_sig_pk = CanisterSigPublicKey::new(canister_id, LOCAL_SEED.to_vec());
        let credential_jwt = String::from_utf8(TEST_CREDENTIAL_JWT.into()).expect("wrong JWT");
        let signing_input = vc_signing_input(&credential_jwt, &canister_sig_pk)
            .expect("failed constructing signing input");
        let extracted_pk = canister_sig_pk_from_vc_signing_input(signing_input.as_slice())
            .expect("failed extracting pk");
        assert_eq!(extracted_pk, canister_sig_pk);
    }

    #[test]
    fn should_compute_icp_did() {
        let principal = dapp_principal();
        let did = did_for_principal(principal);
        assert!(did.starts_with("did:icp:"));
        assert!(did.ends_with(&principal.to_string()));
        assert_eq!(did.len(), "did:icp:".len() + principal.to_string().len());
    }

    #[test]
    fn should_validate_id_alias_claims() {
        let claims = claims_from_jws(ALIAS_JWS);
        validate_claim("iss", II_ISSUER_URL, claims.iss())
            .expect("Failed validating id_alias claims");
    }

    #[test]
    fn should_verify_credential_jws() {
        verify_credential_jws_with_canister_id(
            ALIAS_JWS,
            &mainnet_ii_canister_sig_pk().canister_id,
            &mainnet_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS,
        )
        .expect("JWS verification failed");
    }

    #[test]
    fn should_fail_verify_credential_jws_if_expired() {
        let result = verify_credential_jws_with_canister_id(
            ALIAS_JWS,
            &mainnet_ii_canister_sig_pk().canister_id,
            &&mainnet_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_AFTER_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if e.to_string().contains("credential expired"));
    }

    #[test]
    fn should_fail_verify_credential_jws_if_no_expiry() {
        let result = verify_credential_jws_with_canister_id(
            TEST_CREDENTIAL_JWS_NO_EXPIRY,
            &local_issuer_canister_sig_pk().canister_id,
            &local_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_AFTER_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if e.to_string().contains("structure is not semantically correct"));
    }

    #[test]
    fn should_fail_verify_credential_jws_without_canister_pk() {
        let result = verify_credential_jws_with_canister_id(
            ID_ALIAS_CREDENTIAL_JWS_NO_JWK,
            &mainnet_ii_canister_sig_pk().canister_id,
            &mainnet_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if e.to_string().contains("missing JWK in JWS header"));
    }

    #[test]
    fn should_fail_verify_credential_jws_with_wrong_canister_sig_pk() {
        let wrong_canister_sig_pk = CanisterSigPublicKey::new(alias_principal(), vec![1, 2, 3]);
        let result = verify_credential_jws_with_canister_id(
            ALIAS_JWS,
            &wrong_canister_sig_pk.canister_id,
            &mainnet_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if e.to_string().contains("canister sig canister id does not match provided canister id"));
    }

    #[test]
    fn should_fail_verify_credential_jws_with_wrong_root_pk() {
        let mut ic_root_pk = local_ic_root_pk_raw();
        ic_root_pk[IC_ROOT_PK_DER_PREFIX.len()] += 1; // change the root pk value
        let result = verify_credential_jws_with_canister_id(
            ALIAS_JWS,
            &mainnet_ii_canister_sig_pk().canister_id,
            &local_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        let err = result.err().expect("expected error");
        assert!(err.to_string().contains("invalid BLS signature"))
    }

    #[test]
    fn should_verify_and_extract_id_alias_credential_jws() {
        let alias_tuple = get_verified_id_alias_from_jws(
            ALIAS_JWS,
            &dapp_principal(),
            RP_DERIVATION_ORIGIN_OLD_DOMAIN,
            &mainnet_ii_canister_sig_pk().canister_id,
            &&mainnet_ic_root_pk_raw(),
            ALIAS_CURRENT_TIME_BEFORE_EXPIRY_NS,
        )
        .expect("JWS verification failed");
        assert_eq!(
            alias_tuple,
            AliasTuple {
                id_alias: alias_principal(),
                id_dapp: dapp_principal(),
                derivation_origin: RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            }
        )
    }

    #[test]
    fn should_parse_verifiable_presentation() {
        let id_alias_vc_jws = "a dummy id_alias_vc_jws".to_string();
        let requested_vc_jws = "a dummy requested_vc_jws".to_string();
        let holder = dapp_principal();
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            holder,
            id_alias_vc_jws.clone(),
            requested_vc_jws.clone(),
        )
        .expect("vp-creation failed");
        let credentials = extract_credentials_from_vp(&vp_jwt).expect("failed jwt parsing");

        assert!(credentials.contains(&Jwt::from(id_alias_vc_jws)));
        assert!(credentials.contains(&Jwt::from(requested_vc_jws)));
    }

    #[test]
    fn should_verify_ii_presentation() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let (alias_tuple_from_jws, _claims) = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        )
        .expect("vp verification failed");
        assert_eq!(id_alias, alias_tuple_from_jws.id_alias);
        assert_eq!(id_dapp, alias_tuple_from_jws.id_dapp);
    }

    #[test]
    fn should_verify_ii_presentation_with_new_domain_mainnet_ii() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let (alias_tuple_from_jws, _claims) = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_NEW_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        )
        .expect("vp verification failed");
        assert_eq!(id_alias, alias_tuple_from_jws.id_alias);
        assert_eq!(id_dapp, alias_tuple_from_jws.id_dapp);
    }

    #[test]
    fn should_fail_verify_ii_presentation_if_expired() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_AFTER_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("credential expired"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_extra_vc() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = construct_verifiable_presentation_jwt(
            id_dapp,
            vec![
                VP_ID_ALIAS_JWS.to_string(),
                VP_VC_JWS.to_string(),
                "an extra vc".to_string(),
            ],
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("expected exactly two verifiable credentials"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_missing_vc() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt =
            construct_verifiable_presentation_jwt(id_dapp, vec![VP_ID_ALIAS_JWS.to_string()])
                .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("expected exactly two verifiable credentials"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_wrong_effective_subject() {
        let wrong_subject = alias_principal(); // does not match the "sub" VP_VC_JWS
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            wrong_subject,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            wrong_subject,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("unexpected vc subject"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_non_matching_id_alias_in_vcs() {
        let id_dapp = dapp_principal(); // does match ALIAS_JWS

        // ALIAS_JWS does not match VP_VC_JWS
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("subject does not match id_alias"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_invalid_id_alias_vc() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");

        let mut bad_id_alias_vc = VP_ID_ALIAS_JWS.to_string();
        bad_id_alias_vc.insert(42, 'a');
        let vp_jwt =
            build_ii_verifiable_presentation_jwt(id_dapp, bad_id_alias_vc, VP_VC_JWS.to_string())
                .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("InvalidSignature"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_invalid_requested_vc() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");

        let mut bad_requested_vc = VP_VC_JWS.to_string();
        bad_requested_vc.insert(42, 'a');
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            bad_requested_vc,
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("InvalidSignature"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_wrong_ii_canister_id() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");

        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &VcFlowSigners {
                ii_canister_id: local_issuer_canister_sig_pk().canister_id,
                ..mainnet_test_vc_flow_signers()
            },
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("canister id does not match"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_wrong_issuer_canister_id() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");

        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &VcFlowSigners {
                issuer_canister_id: local_ii_canister_sig_pk().canister_id,
                ..mainnet_test_vc_flow_signers()
            },
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("canister id does not match"));
    }

    #[test]
    fn should_fail_verify_ii_presentation_with_wrong_order_of_vcs() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");

        // Swap the order of the VCs
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_VC_JWS.to_string(),
            VP_ID_ALIAS_JWS.to_string(),
        )
        .expect("vp creation failed");
        let result = verify_ii_presentation_jwt_with_canister_ids(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &VcFlowSigners {
                // Swap also the order of the canister ids, so that they match the VCs
                ii_canister_id: mainnet_issuer_canister_sig_pk().canister_id,
                issuer_canister_id: mainnet_ii_canister_sig_pk().canister_id,
                ..mainnet_test_vc_flow_signers()
            },
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).contains("inconsistent claim in VC"));
    }

    fn credential_spec_with_0_args() -> CredentialSpec {
        CredentialSpec {
            credential_type: "vcWithoutArgs".to_string(),
            arguments: None,
        }
    }

    fn credential_spec_with_1_arg() -> CredentialSpec {
        let mut args = HashMap::new();
        args.insert(
            "firstArg".to_string(),
            ArgumentValue::String("string arg value".to_string()),
        );
        CredentialSpec {
            credential_type: "vcWithOneArg".to_string(),
            arguments: Some(args),
        }
    }

    fn credential_spec_with_2_args() -> CredentialSpec {
        let mut args = HashMap::new();
        args.insert(
            "anotherFirstArg".to_string(),
            ArgumentValue::String("string arg value".to_string()),
        );
        args.insert("secondArg".to_string(), ArgumentValue::Int(42));
        CredentialSpec {
            credential_type: "vcWithTwoArgs".to_string(),
            arguments: Some(args),
        }
    }

    fn credential_specs_for_test() -> Vec<CredentialSpec> {
        vec![
            credential_spec_with_0_args(),
            credential_spec_with_1_arg(),
            credential_spec_with_2_args(),
        ]
    }

    fn vc_claims_for_spec(spec: &CredentialSpec) -> Map<String, Value> {
        let mut claims = Map::new();
        let types = vec![
            Value::String("VerifiableCredential".to_string()),
            Value::String(spec.credential_type.to_string()),
        ];
        claims.insert("type".to_string(), Value::Array(types));
        let mut arguments = Map::new();
        if let Some(args) = spec.arguments.as_ref() {
            for arg in args {
                arguments.insert(arg.0.clone(), arg.1.clone().into());
            }
        }
        let mut subject = Map::new();
        subject.insert(spec.credential_type.clone(), Value::Object(arguments));
        claims.insert("credentialSubject".to_string(), Value::Object(subject));
        claims
    }

    #[test]
    fn should_validate_claims_match_spec() {
        for spec in credential_specs_for_test() {
            let claims = vc_claims_for_spec(&spec);
            validate_claims_match_spec(&claims, &spec)
                .unwrap_or_else(|_| panic!("failed for spec: {:?}", spec));
        }
    }

    #[test]
    fn should_fail_validate_claims_match_spec_if_wrong_type() {
        for spec in credential_specs_for_test() {
            // Construct claims with wrong "type" entry.
            let mut claims = vc_claims_for_spec(&spec);
            claims.insert(
                "type".to_string(),
                Value::Array(vec![Value::String("WrongType".to_string())]),
            );
            let result = validate_claims_match_spec(&claims, &spec);
            assert_matches!(result, Err(e) if format!("{:?}", e).contains("missing credential_type in type-claim"));
        }
    }

    #[test]
    fn should_fail_validate_claims_match_spec_if_missing_credential_type_claim() {
        for spec in credential_specs_for_test() {
            // Construct claims without "credential_type"-claim.
            let mut claims = vc_claims_for_spec(&spec);
            claims
                .get_mut("credentialSubject")
                .expect("missing credentialSubject")
                .as_object_mut()
                .expect("wrong credentialSubject")
                .remove(&spec.credential_type)
                .expect("missing credential_type claim");
            let result = validate_claims_match_spec(&claims, &spec);
            assert_matches!(result, Err(e) if format!("{:?}", e).contains("missing credential_type claim"));
        }
    }

    #[test]
    fn should_fail_validate_claims_match_spec_with_extra_args_in_credential_type_claim() {
        for spec in credential_specs_for_test() {
            // Construct claims with extra arg in "credential_type"-claim.
            let mut claims = vc_claims_for_spec(&spec);
            claims
                .get_mut("credentialSubject")
                .expect("missing credentialSubject")
                .as_object_mut()
                .expect("wrong credentialSubject")
                .get_mut(&spec.credential_type)
                .expect("missing credential_type claim")
                .as_object_mut()
                .expect("wrong credential_type claim")
                .insert("extraArg".to_string(), Value::Null);
            let result = validate_claims_match_spec(&claims, &spec);
            assert_matches!(result, Err(e) if format!("{:?}", e).contains("wrong number of credential_type arguments"));
        }
    }

    #[test]
    fn should_fail_validate_claims_match_spec_with_missing_args_in_credential_type_claim() {
        for spec in [credential_spec_with_1_arg(), credential_spec_with_2_args()] {
            // Construct claims with extra arg in "credential_type"-claim.
            let mut claims = vc_claims_for_spec(&spec);
            let arg_name = spec.arguments.as_ref().unwrap().keys().last().unwrap();
            claims
                .get_mut("credentialSubject")
                .expect("missing credentialSubject")
                .as_object_mut()
                .expect("wrong credentialSubject")
                .get_mut(&spec.credential_type)
                .expect("missing credential_type claim")
                .as_object_mut()
                .expect("wrong credential_type claim")
                .remove(arg_name);
            let result = validate_claims_match_spec(&claims, &spec);
            assert_matches!(result, Err(e) if format!("{:?}", e).contains("wrong number of credential_type arguments"));
        }
    }

    #[test]
    fn should_fail_validate_claims_match_spec_with_wrong_arg_value_in_credential_type_claim() {
        for spec in [credential_spec_with_1_arg(), credential_spec_with_2_args()] {
            // Construct claims with extra arg in "credential_type"-claim.
            let mut claims = vc_claims_for_spec(&spec);
            let arg_name = spec.arguments.as_ref().unwrap().keys().last().unwrap();
            claims
                .get_mut("credentialSubject")
                .expect("missing credentialSubject")
                .as_object_mut()
                .expect("wrong credentialSubject")
                .get_mut(&spec.credential_type)
                .expect("missing credential_type claim")
                .as_object_mut()
                .expect("wrong credential_type claim")
                .insert(arg_name.clone(), Value::String("a wrong value".to_string()));
            let result = validate_claims_match_spec(&claims, &spec);
            assert_matches!(result, Err(e) if format!("{:?}", e).contains("wrong value in credential_type argument"));
        }
    }

    fn vp_vc_spec() -> CredentialSpec {
        let mut args = HashMap::new();
        args.insert("one".to_string(), ArgumentValue::String("arg".to_string()));
        CredentialSpec {
            credential_type: "Test".to_string(),
            arguments: Some(args),
        }
    }

    #[test]
    fn should_validate_ii_presentation_and_claims() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp-creation failed");
        validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        )
        .expect("VP verification failed");
    }

    #[test]
    fn should_fail_validate_ii_presentation_and_claims_if_wrong_vc_flow_signers() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp-creation failed");

        // wrong ii_canister_id
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &VcFlowSigners {
                ii_canister_id: mainnet_issuer_canister_sig_pk().canister_id,
                ..mainnet_test_vc_flow_signers()
            },
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("InvalidSignature"));

        // wrong issuer_canister_id
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &VcFlowSigners {
                issuer_canister_id: mainnet_ii_canister_sig_pk().canister_id,
                ..mainnet_test_vc_flow_signers()
            },
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("InvalidSignature"));

        // wrong issuer_origin
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &VcFlowSigners {
                issuer_origin: "https://wrong.origin.com".to_string(),
                ..mainnet_test_vc_flow_signers()
            },
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("InconsistentCredentialJwtClaims"));
    }

    #[test]
    fn should_fail_validate_ii_presentation_and_claims_if_wrong_effective_subject() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp-creation failed");
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_alias, // wrong effective subject
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("unexpected vc subject"));
    }

    #[test]
    fn should_fail_validate_ii_presentation_and_claims_if_expired() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp-creation failed");
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_AFTER_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("credential expired"));
    }

    #[test]
    fn should_fail_validate_ii_presentation_and_claims_if_wrong_spec() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp-creation failed");
        let wrong_spec = CredentialSpec {
            credential_type: "NotSameCredential".to_string(),
            arguments: None,
        };
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &mainnet_test_vc_flow_signers(),
            &wrong_spec,
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("InconsistentCredentialJwtClaims"));
    }

    #[test]
    fn should_fail_validate_ii_presentation_and_claims_if_different_derivation_origin() {
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            VP_ID_ALIAS_JWS.to_string(),
            VP_VC_JWS.to_string(),
        )
        .expect("vp-creation failed");
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            "not-same-derivation_origin".to_string(),
            &mainnet_test_vc_flow_signers(),
            &vp_vc_spec(),
            &mainnet_ic_root_pk_raw(),
            VP_CURRENT_TIME_BEFORE_EXPIRY_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("unexpected derivation origin"));
    }

    #[test]
    fn should_fail_validate_ii_presentation_and_claims_if_missing_derivation_origin() {
        let id_alias_no_derivation_origin: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQW9BQUFBQUFBQUFBQUVCMGd6TTVJeXFMYUhyMDhtQTRWd2J5SmRxQTFyRVFUX2xNQnVVbmN5UDVVYyJ9LCJraWQiOiJkaWQ6aWNwOnJ3bGd0LWlpYWFhLWFhYWFhLWFhYWFhLWNhaSIsImFsZyI6IkljQ3MifQ.eyJleHAiOjE2MjAzMjk1MzAsImlzcyI6Imh0dHBzOi8vaWRlbnRpdHkuaWMwLmFwcC8iLCJuYmYiOjE2MjAzMjg2MzAsImp0aSI6ImRhdGE6dGV4dC9wbGFpbjtjaGFyc2V0PVVURi04LHRpbWVzdGFtcF9uczoxNjIwMzI4NjMwMDAwMDAwMDAwLGFsaWFzX2hhc2g6NThiYzcxMmYyMjFhOTJmMGE5OTRhZDZmN2JmOWVjNjc0MzBmMGFkMzNmYWVlZDAzZmUzZDU2NTYyMTliMjQ2MiIsInN1YiI6ImRpZDppY3A6cDJubGMtM3M1dWwtbGN1NzQtdDZwbjItdWk1aW0taTRhNWYtYTR0Z2EtZTZ6bmYtdG52bGgtd2ttanMtZHFlIiwidmMiOnsiQGNvbnRleHQiOiJodHRwczovL3d3dy53My5vcmcvMjAxOC9jcmVkZW50aWFscy92MSIsInR5cGUiOlsiVmVyaWZpYWJsZUNyZWRlbnRpYWwiLCJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyJdLCJjcmVkZW50aWFsU3ViamVjdCI6eyJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyI6eyJoYXNJZEFsaWFzIjoiamtrMjItenFkeGMta2dwZXotNnN2Mm0tNXBieTQtd2k0dDItcHJtb3EtZ2YyaWgtaTJxdGMtdjM3YWMtNWFlIn19fX0.2dn3omtjZXJ0aWZpY2F0ZVkBsdnZ96JkdHJlZYMBgwGDAYMCSGNhbmlzdGVygwGDAkoAAAAAAAAAAAEBgwGDAYMBgwJOY2VydGlmaWVkX2RhdGGCA1ggvlJBTZDgK1_9Vb3-18dWKIfy28WTjZ1YqdjFWWAIX96CBFgg0sz_P8xdqTDewOhKJUHmWFFrS7FQHnDotBDmmGoFfWCCBFgg_KZ0TVqubo_EGWoMUPA35BYZ4B5ZRkR_zDfNIQCwa46CBFggDxSoL5vzjhHDgnrdmgRhclanMmjjpWYL41-us6gEU6mCBFggXAzCWvb9h4qsVs41IUJBABzjSqAZ8DIzF_ghGHpGmHGCBFggRbE3sOaqi_9kL-Uz1Kmf_pCWt4FSRaHU9KLSFTT3eceCBFggQERIfN1eHBUYfQr2fOyI_nTKHS71uqu-wOAdYwqyUX-DAYIEWCA1U_ZYHVOz3Sdkb2HIsNoLDDiBuFfG3DxH6miIwRPra4MCRHRpbWWCA0mAuK7U3YmkvhZpc2lnbmF0dXJlWDCm_9R-rt9zbE2eP_WbCyFqO7txO86wNfBS1lyyJJ6gxy1D2Wnw5kNo2XUKUBmu9q5kdHJlZYMBggRYIOGnlc_3yXPTVrEJ1p3dKX5HxkMOziUnpA1HeXiQW4O8gwJDc2lngwJYIIOQR7wl3Ws9Jb8VP4rhIb37XKLMkkZ2P7WaZ5we60WGgwGDAlgg3DSOKS3cc99bdJqFjiOcs13PNpGSR8_5-UJsP23Ud0KCA0CCBFgg6wJlRmEtuY-LCp6ieeEdd6tO8_Hlct7H8VrW9DH7EaI";
        "zj7fh-j3cgv-eojdr-h55ha-4o67r-32utu-22iv2-o7hhy-3yoot-xdcrf-5qe";
        let vc_for_no_derivation_origin: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQW9BQUFBQUFBQUFBUUVCOEVpSWoyNkJxRWhic2ZQUW44TF9CNDJxc0JOeUdiT3ZLdlNENE9OUGhsSSJ9LCJraWQiOiJkaWQ6aWNwOnJya2FoLWZxYWFhLWFhYWFhLWFhYWFxLWNhaSIsImFsZyI6IkljQ3MifQ.eyJleHAiOjE2MjAzMjk1MzAsImlzcyI6Imh0dHBzOi8vYWdlX3ZlcmlmaWVyLmluZm8vIiwibmJmIjoxNjIwMzI4NjMwLCJqdGkiOiJodHRwczovL2FnZV92ZXJpZmllci5pbmZvL2NyZWRlbnRpYWxzLzQyIiwic3ViIjoiZGlkOmljcDpqa2syMi16cWR4Yy1rZ3Blei02c3YybS01cGJ5NC13aTR0Mi1wcm1vcS1nZjJpaC1pMnF0Yy12MzdhYy01YWUiLCJ2YyI6eyJAY29udGV4dCI6Imh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIiwidHlwZSI6WyJWZXJpZmlhYmxlQ3JlZGVudGlhbCIsIlZlcmlmaWVkQWR1bHQiXSwiY3JlZGVudGlhbFN1YmplY3QiOnsiVmVyaWZpZWRBZHVsdCI6eyJtaW5BZ2UiOjE4fX19fQ.2dn3omtjZXJ0aWZpY2F0ZVkBsdnZ96JkdHJlZYMBgwGDAYMCSGNhbmlzdGVygwGCBFggOnw-lESEpV-y1s0Lh9p1aY-XfYKBYzyHL_fmcTqp6PeDAkoAAAAAAAAAAQEBgwGDAYMBgwJOY2VydGlmaWVkX2RhdGGCA1ggO8I7YzRNmk_XVakRhuaOq1rdEj3vhLFt07YEWKwrfBSCBFgg0sz_P8xdqTDewOhKJUHmWFFrS7FQHnDotBDmmGoFfWCCBFggsN7BWldXUVrLUx_990beUdGHvTn5XEjFcgTxb8oXZZCCBFggNtRXohqxK3P8d6uyzQLSdJLBe5kv-Ng0gEHSR-OUmryCBFggiOn-4gDlCnp9jkq0VFtcJQPETxg1HnHwdHOddTpIlzWCBFggjFoCQNnMC4FEG3e2zATPdOyzWTcfRqu16bVgC18EQiCDAYIEWCA1U_ZYHVOz3Sdkb2HIsNoLDDiBuFfG3DxH6miIwRPra4MCRHRpbWWCA0mAuK7U3YmkvhZpc2lnbmF0dXJlWDCrcgY2ne3OillJ6fz8uv6dhCykfT-u0ZSKyvXZVYS1zOtRCMOSYZju2k-LERBCmLNkdHJlZYMBggRYIJIlUxoU2qt4aTwzz90fB43OK9EFDzVls4N8OHepeuLpgwJDc2lngwJYIKhCHifwHS5DiNAL6bducWQ2AShCc2bN-TzPsBEl3ov2gwGCBFggCr5Roa_ACiP36lIIHDtA47bq8L7C_nH3Z0GGJrLnE6uDAYMCWCCzsKpLUCoF4k5X0pGLjWSca9QaCMj6-oXkkFtUO7kYtoIDQIIEWCBrqYIFsKJT6MmiyQ79ksiXynSLIxl4HdOrpgsXm4TVBw";
        let id_dapp =
            Principal::from_text("p2nlc-3s5ul-lcu74-t6pn2-ui5im-i4a5f-a4tga-e6znf-tnvlh-wkmjs-dqe")
                .expect("wrong principal");
        let expiry_ns: u128 = 1620329530 * 1_000_000_000;
        let mut args = HashMap::new();
        args.insert("minAge".to_string(), ArgumentValue::Int(18));
        let spec = CredentialSpec {
            credential_type: "VerifiedAdult".to_string(),
            arguments: Some(args),
        };
        let flow_signers = VcFlowSigners {
            ii_canister_id: local_ii_canister_sig_pk().canister_id,
            ii_origin: II_ISSUER_URL.to_string(),
            issuer_canister_id: local_issuer_canister_sig_pk().canister_id,
            issuer_origin: "https://age_verifier.info/".to_string(),
        };
        let vp_jwt = build_ii_verifiable_presentation_jwt(
            id_dapp,
            id_alias_no_derivation_origin.to_string(),
            vc_for_no_derivation_origin.to_string(),
        )
        .expect("vp-creation failed");
        let result = validate_ii_presentation_and_claims(
            &vp_jwt,
            id_dapp,
            RP_DERIVATION_ORIGIN_OLD_DOMAIN.to_string(),
            &flow_signers,
            &spec,
            &local_ic_root_pk_raw(),
            expiry_ns - MINUTE_NS,
        );
        assert_matches!(result, Err(e) if format!("{:?}", e).to_string().contains("missing \\\"derivationOrigin\\\" parameter in id_alias JWT vc"));
    }

    // Removes nbf-entry from the given VC-JWT.
    fn remove_nbf(vc_jwt: &str) -> String {
        let mut ret = vc_jwt.to_string();
        let nbf_start = vc_jwt.find("\"nbf\"").unwrap();
        let nbf_end = vc_jwt.find("\"jti\"").unwrap();
        ret.replace_range(nbf_start..nbf_end, "");
        ret
    }

    #[test]
    fn should_build_credential_jwt() {
        // Should match the "exp" in the example
        let expiration = 1620329470;
        // Should match the "sub" in the example
        let id_dapp_text = "p2nlc-3s5ul-lcu74-t6pn2-ui5im-i4a5f-a4tga-e6znf-tnvlh-wkmjs-dqe";
        // Should match the credentials of the example jwt
        let mut args = HashMap::new();
        args.insert("minAge".to_string(), ArgumentValue::Int(18));
        let spec = CredentialSpec {
            credential_type: "VerifiedAdult".to_string(),
            arguments: Some(args),
        };
        let example_jwt = "{\"exp\":1620329470,\"iss\":\"https://age_verifier.info/\",\"nbf\":1707817485,\"jti\":\"https://age_verifier.info/credentials/42\",\"sub\":\"did:icp:p2nlc-3s5ul-lcu74-t6pn2-ui5im-i4a5f-a4tga-e6znf-tnvlh-wkmjs-dqe\",\"vc\":{\"@context\":\"https://www.w3.org/2018/credentials/v1\",\"type\":[\"VerifiableCredential\",\"VerifiedAdult\"],\"credentialSubject\":{\"VerifiedAdult\":{\"minAge\":18}}}}";
        let example_jwt_without_nbf = "{\"exp\":1620329470,\"iss\":\"https://age_verifier.info/\",\"jti\":\"https://age_verifier.info/credentials/42\",\"sub\":\"did:icp:p2nlc-3s5ul-lcu74-t6pn2-ui5im-i4a5f-a4tga-e6znf-tnvlh-wkmjs-dqe\",\"vc\":{\"@context\":\"https://www.w3.org/2018/credentials/v1\",\"type\":[\"VerifiableCredential\",\"VerifiedAdult\"],\"credentialSubject\":{\"VerifiedAdult\":{\"minAge\":18}}}}";
        let id_dapp = Principal::from_text(id_dapp_text).expect("wrong principal");
        let params = CredentialParams {
            spec,
            subject_id: did_for_principal(id_dapp),
            credential_id_url: "https://age_verifier.info/credentials/42".to_string(),
            issuer_url: "https://age_verifier.info".to_string(),
            expiration_timestamp_s: expiration,
        };
        let credential = build_credential_jwt(params);
        assert_eq!(credential.len(), example_jwt.len());
        // First check that the built credential differs from the example one (they have different nbf-entries).
        assert_ne!(credential, example_jwt);
        // After the removal of the nbf-entries, all the remaining information should be identical.
        assert_eq!(remove_nbf(credential.as_str()), example_jwt_without_nbf);
    }

    // Tests for `matches_expected_origin`

    #[test]
    fn test_matches_expected_origin_custom_domain() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::anonymous();
        let expected_derivation_origin = "https://custom-domain.com";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://custom-domain.com".to_string(),
        };

        assert!(matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }

    #[test]
    fn test_matches_expected_origin_ii_legacy_domain() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::from_text(II_MAINNET).unwrap();
        let expected_derivation_origin = "https://example.icp0.io";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://example.ic0.app".to_string(),
        };

        assert!(matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }

    #[test]
    fn test_matches_expected_origin_new_domain_no_ii() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::anonymous();
        let expected_derivation_origin = "https://example.icp0.io";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://example.icp0.io".to_string(),
        };

        assert!(matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }

    #[test]
    fn test_does_not_match_expected_origin_new_domain_ii_mainnet() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::from_text(II_MAINNET).unwrap();
        let expected_derivation_origin = "https://example.icp0.io";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://example.icp0.io".to_string(),
        };

        assert!(!matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }

    #[test]
    fn test_does_not_match_different_domains() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::anonymous();
        let expected_derivation_origin = "https://another-domain.com";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://different-domain.com".to_string(),
        };

        assert!(!matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }

    #[test]
    fn test_does_not_match_different_canister_domains() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::anonymous();
        let expected_derivation_origin = "https://aaaaa-aa.ic0.app";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://rrkah-fqaaa-aaaaa-aaaaq-cai.ic0.app".to_string(),
        };

        assert!(!matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }

    #[test]
    fn test_matches_expected_origin_subdomain_with_raw() {
        let id_alias = Principal::from_text(VP_ID_ALIAS).expect("wrong principal");
        let id_dapp = Principal::from_text(VP_RP_ID).expect("wrong principal");
        let signing_canister_id = Principal::from_text(II_MAINNET).unwrap();
        let expected_derivation_origin = "https://example.raw.icp0.io";
        let alias_tuple = AliasTuple {
            id_alias,
            id_dapp,
            derivation_origin: "https://example.raw.ic0.app".to_string(),
        };

        assert!(matches_expected_origin(
            &signing_canister_id,
            expected_derivation_origin,
            &alias_tuple
        ));
    }
}
