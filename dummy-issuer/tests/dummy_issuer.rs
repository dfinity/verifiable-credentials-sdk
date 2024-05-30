use candid::{decode_one, encode_one, CandidType, Deserialize, Principal};
use pocket_ic::{PocketIc, WasmResult};
use vc_util::issuer_api::{
    CredentialSpec, Icrc21ConsentInfo, Icrc21ConsentPreferences, Icrc21Error,
    Icrc21VcConsentMessageRequest,
};

pub const DUMMY_ISSUER_WASM: &[u8] = include_bytes!("../dummy_issuer.wasm.gz");

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum VariantResponse<T, E> {
    Ok(T),
    Err(E),
}

#[test]
fn test_consent_message() {
    let pic = PocketIc::new();
    // Create an empty canister as the anonymous principal and add cycles.
    let canister_id = pic.create_canister();
    pic.add_cycles(canister_id, 2_000_000_000_000);
    pic.install_canister(canister_id, DUMMY_ISSUER_WASM.to_vec(), vec![], None);

    let request = Icrc21VcConsentMessageRequest {
        credential_spec: CredentialSpec {
            credential_type: "Test".to_string(),
            arguments: None,
        },
        preferences: Icrc21ConsentPreferences {
            language: "en".to_string(),
        },
    };

    let consent_message_reply = pic
        .update_call(
            canister_id,
            Principal::anonymous(),
            "vc_consent_message",
            encode_one(request).unwrap(),
        )
        .expect("Error calling canister");
    let WasmResult::Reply(reply) = consent_message_reply else {
        unreachable!()
    };
    println!("after update_call");
    let response: VariantResponse<Icrc21ConsentInfo, Icrc21Error> = decode_one(&reply).unwrap();
    match response {
        VariantResponse::Ok(Icrc21ConsentInfo {
            consent_message,
            language: _lang,
        }) => {
            assert_eq!(consent_message, "Consent message from dummy issuer: Test");
        }
        VariantResponse::Err(_) => panic!("Failed to call consent_message"),
    }
}
