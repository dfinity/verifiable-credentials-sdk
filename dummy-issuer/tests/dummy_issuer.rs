use candid::{decode_one, encode_one, CandidType, Deserialize, Principal};
use ic_cdk::api::management_canister::main::CanisterId;
use pocket_ic::{PocketIc, WasmResult};
use serde::de::DeserializeOwned;
use vc_util::issuer_api::{
    CredentialSpec, DerivationOriginData, DerivationOriginError, DerivationOriginRequest,
    Icrc21ConsentInfo, Icrc21ConsentPreferences, Icrc21Error, Icrc21VcConsentMessageRequest,
};

pub const DUMMY_ISSUER_WASM: &[u8] = include_bytes!("../dummy_issuer.wasm.gz");

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

mod api {
    use super::*;

    fn call_canister<'a, Req, Succ, Err>(
        pic: &PocketIc,
        method: &str,
        canister_id: CanisterId,
        request: Req,
        sender: Option<Principal>,
    ) -> Result<Succ, Err>
    where
        Req: CandidType,
        Succ: CandidType + DeserializeOwned,
        Err: CandidType + DeserializeOwned,
    {
        let consent_message_reply = pic
            .update_call(
                canister_id,
                sender.unwrap_or(Principal::anonymous()),
                method,
                encode_one(request).unwrap(),
            )
            .expect("Error calling canister");
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
            assert_eq!(consent_message, "Consent message from dummy issuer: Test");
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
