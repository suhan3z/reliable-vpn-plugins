use rvpn_provider_sdk::*;
pub fn dispatch(call: Call) -> Step {
    match call.method {
        Method::Describe => {
            complete(serde_json::from_str::<Manifest>(include_str!("../manifest.json")).unwrap())
        }
        Method::ListLocations | Method::ListOffers => complete(serde_json::json!([])),
        _ => error("unsupported"),
    }
}
rvpn_provider_sdk::export_provider!(dispatch);
