use base64::Engine;
use std::collections::HashMap;

use candid::{decode_one, encode_one, CandidType, Deserialize, Principal};
use ic_cdk::api::management_canister::main::CanisterId;
use ic_verifiable_credentials::issuer_api::{
    ArgumentValue, CredentialSpec, DerivationOriginData, DerivationOriginError,
    DerivationOriginRequest, GetCredentialRequest, Icrc21ConsentInfo, Icrc21ConsentPreferences,
    Icrc21Error, Icrc21VcConsentMessageRequest, PrepareCredentialRequest, SignedIdAlias,
};
use pocket_ic::{PocketIc, WasmResult};
use serde::de::DeserializeOwned;
use serde_json::Value;

const DUMMY_ISSUER_WASM: &[u8] = include_bytes!("../dummy_issuer.wasm.gz");
const ID_ALIAS_JWT: &str = "eyJqd2siOnsia3R5Ijoib2N0IiwiYWxnIjoiSWNDcyIsImsiOiJNRHd3REFZS0t3WUJCQUdEdUVNQkFnTXNBQXFBQUFBQUFCQUFGZ0VCVko4aGgwR2xBTmFMdUtRVGNZWTlwa01WVFhPLTMzaEctY0tyaHVkaTZ3cyJ9LCJraWQiOiJkaWQ6aWNwOmNwbWNyLXllYWFhLWFhYWFhLXFhYWxhLWNhaSIsImFsZyI6IkljQ3MifQ.eyJleHAiOjE3MTc1MDAwOTcsImlzcyI6Imh0dHBzOi8vaWRlbnRpdHkuaWMwLmFwcC8iLCJuYmYiOjE3MTc0OTkxOTcsImp0aSI6ImRhdGE6dGV4dC9wbGFpbjtjaGFyc2V0PVVURi04LHRpbWVzdGFtcF9uczoxNzE3NDk5MTk3NjkxMjUzMDAwLGFsaWFzX2hhc2g6ZWJjOThmYTk2NDFlZGIwYTY3ZGEwYjBkZjExZDIyZjVjNDRjYTNlNWI2OWM5MTA0NTA4M2FkNzY5NmNmMjQ4NSIsInN1YiI6ImRpZDppY3A6MmRyN2ItZHkyN28tYXQzbDQtdGlra2otamdmNWYtYjRxb2gtbzNpcWQtcWdmN2ktYnhpeWUtenpmaWUtbGFlIiwidmMiOnsiQGNvbnRleHQiOiJodHRwczovL3d3dy53My5vcmcvMjAxOC9jcmVkZW50aWFscy92MSIsInR5cGUiOlsiVmVyaWZpYWJsZUNyZWRlbnRpYWwiLCJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyJdLCJjcmVkZW50aWFsU3ViamVjdCI6eyJJbnRlcm5ldElkZW50aXR5SWRBbGlhcyI6eyJoYXNJZEFsaWFzIjoieHA3bWYtaWR6eTYtaHMzM2cteGc2Z3ota25henktdG8yM3EtdHVnZTYtN2JoNmEtanp0bXctYjNwd3QtZWFlIn19fX0.2dn3omtjZXJ0aWZpY2F0ZVkB19nZ96JkdHJlZYMBgwGDAYMCSGNhbmlzdGVygwGCBFggWoRx2PbCEeN0ixn7e-UirzJAHQY9r9kyhb3SnPxBP4uDAYIEWCBMONzHAnK0jVuK997XJV_6hFZbaBWN0KTUmlYR3WWXuoMBggRYIDi4Wsz22ukd8m0kIdYCk9K2rg70THv5w85DEDpYdZIDgwJKgAAAAAAQABYBAYMBgwGDAk5jZXJ0aWZpZWRfZGF0YYIDWCA_rx5TB6eC52CeEdXy4s34iY3s2EASfBqBFcPS9fH3uYIEWCBatD7fWrBBUJYAaHUYRNPKsGB2BCCknoh1Rkwqf-_CaoIEWCAcUK9eLRSw46lWjStyyRFOKRFUS7OBv0QxoMpeALgGdoIEWCAK7Ec4DvTqpmpE9JHYsT8FHSrfNiKVnu3yVlMU-6KxIYIEWCCQkT1Z3skTRzJUOWzrPTf_sBu5aZ6qr88jo8smnm6f_YMBggRYIEMI93i492dWsJprkB2UAvBYtBIysPetVVgxHc4T-hWDgwJEdGltZYIDScDeppy9jfLqF2lzaWduYXR1cmVYMKd5pfn-heKQin4SIIfx8m0q7zYdhEHYVIxuYOBaAF3-ufINwggmfZ1Zksa22lTCRWR0cmVlgwGCBFgglmDq7rrtAl5ZQOMxkfGbeb5IVvzUoR--PM8Xn7FF7SCDAkNzaWeDAYIEWCCDC09GDBV0Srb1Wq3RvbhIEva9o85g64EBa50fPSKTN4MCWCDY4NRNLSFMY8yUHhJMPqTKnNY9KWJdPlHJyeuTexz8HYMBgwJYIBoUAasHcdl_6m08nzzRfhIlxxAp3PNHf9xhI3E9wIkmggNAggRYIOkldmMiQ8kGhCHGzH6xlfCbGo7cpFVzoEJVpg204zCc";

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum VariantResponse<T, E> {
    Ok(T),
    Err(E),
}

fn install_issuer_canister(pic: &PocketIc) -> Principal {
    // Create an empty canister as the anonymous principal and add cycles.
    let canister_id = pic.create_canister();
    pic.add_cycles(canister_id, 2_000_000_000_000);
    pic.install_canister(canister_id, DUMMY_ISSUER_WASM.to_vec(), vec![], None);
    canister_id
}

enum CanisterCall {
    Query,
    Update,
}

mod api {
    use ic_verifiable_credentials::issuer_api::{
        GetCredentialRequest, IssueCredentialError, IssuedCredentialData, PrepareCredentialRequest,
        PreparedCredentialData,
    };

    use super::*;

    fn call_canister<'a, Req, Succ, Err>(
        pic: &PocketIc,
        method: &str,
        call_type: CanisterCall,
        canister_id: CanisterId,
        request: Req,
        sender: Option<Principal>,
    ) -> Result<Succ, Err>
    where
        Req: CandidType,
        Succ: CandidType + DeserializeOwned,
        Err: CandidType + DeserializeOwned,
    {
        let consent_message_reply = match call_type {
            CanisterCall::Query => pic
                .query_call(
                    canister_id,
                    sender.unwrap_or(Principal::anonymous()),
                    method,
                    encode_one(request).unwrap(),
                )
                .expect("Error calling canister"),
            CanisterCall::Update => pic
                .update_call(
                    canister_id,
                    sender.unwrap_or(Principal::anonymous()),
                    method,
                    encode_one(request).unwrap(),
                )
                .expect("Error calling canister"),
        };
        let WasmResult::Reply(reply) = consent_message_reply else {
            unreachable!()
        };
        let response: VariantResponse<Succ, Err> = decode_one(&reply).unwrap();
        match response {
            VariantResponse::Ok(success) => Ok(success),
            VariantResponse::Err(err) => Err(err),
        }
    }

    pub fn consent_message(
        pic: &PocketIc,
        canister_id: CanisterId,
        request: Icrc21VcConsentMessageRequest,
        sender: Option<Principal>,
    ) -> Result<Icrc21ConsentInfo, Icrc21Error> {
        call_canister::<Icrc21VcConsentMessageRequest, Icrc21ConsentInfo, Icrc21Error>(
            pic,
            "vc_consent_message",
            CanisterCall::Update,
            canister_id,
            request,
            sender,
        )
    }

    pub fn derivation_origin(
        pic: &PocketIc,
        canister_id: CanisterId,
        request: DerivationOriginRequest,
        sender: Option<Principal>,
    ) -> Result<DerivationOriginData, DerivationOriginError> {
        call_canister::<DerivationOriginRequest, DerivationOriginData, DerivationOriginError>(
            pic,
            "derivation_origin",
            CanisterCall::Update,
            canister_id,
            request,
            sender,
        )
    }

    pub fn prepare_credential(
        pic: &PocketIc,
        canister_id: CanisterId,
        request: PrepareCredentialRequest,
        sender: Option<Principal>,
    ) -> Result<PreparedCredentialData, IssueCredentialError> {
        call_canister::<PrepareCredentialRequest, PreparedCredentialData, IssueCredentialError>(
            pic,
            "prepare_credential",
            CanisterCall::Update,
            canister_id,
            request,
            sender,
        )
    }

    pub fn get_credential(
        pic: &PocketIc,
        canister_id: CanisterId,
        request: GetCredentialRequest,
        sender: Option<Principal>,
    ) -> Result<IssuedCredentialData, IssueCredentialError> {
        call_canister::<GetCredentialRequest, IssuedCredentialData, IssueCredentialError>(
            pic,
            "get_credential",
            CanisterCall::Query,
            canister_id,
            request,
            sender,
        )
    }
}

#[test]
fn test_consent_message() {
    let pic = PocketIc::new();
    let canister_id = install_issuer_canister(&pic);

    let request = Icrc21VcConsentMessageRequest {
        credential_spec: CredentialSpec {
            credential_type: "Test".to_string(),
            arguments: None,
        },
        preferences: Icrc21ConsentPreferences {
            language: "en".to_string(),
        },
    };

    let response = api::consent_message(&pic, canister_id, request, None);
    match response {
        Ok(Icrc21ConsentInfo {
            consent_message,
            language: _lang,
        }) => {
            assert_eq!(
                consent_message,
                "# Credential Type\nTest\n## Arguments\nNone\n"
            );
        }
        Err(_) => panic!("Failed to call consent_message"),
    }
}

#[test]
fn test_derivation_origin() {
    let pic = PocketIc::new();
    let canister_id = install_issuer_canister(&pic);

    let request_fe_hostname = "http://demo-issuer.vc".to_string();
    let request = DerivationOriginRequest {
        frontend_hostname: request_fe_hostname.clone(),
    };

    let response = api::derivation_origin(&pic, canister_id, request, None);
    match response {
        Ok(DerivationOriginData { origin }) => {
            assert_eq!(origin, request_fe_hostname);
        }
        Err(_) => panic!("Failed to call derivation_origin"),
    }
}

/// Decodes a Verifiable Credential JWT and returns the value within `vc.credentialSubject`.
/// This function doesn't perform any validation or signature verification.
fn get_credential_subject_from_jwt(jwt_vc: String) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
    let payload = jwt_vc
        .split('.')
        .skip(1)
        .next()
        .expect("Failed to parse JWT");
    let claims: Value =
        serde_json::from_slice(&BASE64.decode(payload).expect("Failed to decode base64"))
            .expect("Failed to parse JSON");
    claims
        .pointer("/vc/credentialSubject")
        .expect("Failed to extract credentialSubject")
        .to_string()
}

#[test]
fn should_issue_any_credential() {
    let pic = PocketIc::new();
    let issuer_canister_id = install_issuer_canister(&pic);
    let credential_type = "VerifiedAge".to_string();
    let mut args = HashMap::new();
    args.insert("ageAtLeast".to_string(), ArgumentValue::Int(18));
    let credential_spec = CredentialSpec {
        credential_type: credential_type.clone(),
        arguments: Some(args),
    };
    let signed_id_alias = SignedIdAlias {
        credential_jws: ID_ALIAS_JWT.to_string(),
    };
    let prepare_credential_request = PrepareCredentialRequest {
        signed_id_alias: SignedIdAlias {
            credential_jws: ID_ALIAS_JWT.to_string(),
        },
        credential_spec: credential_spec.clone(),
    };

    let prepared_context_response =
        api::prepare_credential(&pic, issuer_canister_id, prepare_credential_request, None)
            .unwrap();

    let get_credential_request = GetCredentialRequest {
        signed_id_alias,
        credential_spec: credential_spec.clone(),
        prepared_context: prepared_context_response.prepared_context,
    };
    let get_credential_response =
        api::get_credential(&pic, issuer_canister_id, get_credential_request, None).unwrap();

    let vc_jwt = get_credential_subject_from_jwt(get_credential_response.vc_jws);

    assert_eq!(vc_jwt, "{\"VerifiedAge\":{\"ageAtLeast\":18}}");
}
