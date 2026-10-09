use super::*;
fn reply(body: Value) -> HttpResponse {
    HttpResponse { status: 200, body }
}
fn absent() -> HttpResponse {
    HttpResponse {
        status: 404,
        body: Value::Null,
    }
}
fn catalog() -> Vec<HttpResponse> {
    vec![
        reply(serde_json::from_str(include_str!("../fixtures/presets.json")).unwrap()),
        reply(serde_json::from_str(include_str!("../fixtures/locations.json")).unwrap()),
        reply(serde_json::from_str(include_str!("../fixtures/additional.json")).unwrap()),
    ]
}
fn owned() -> Value {
    json!({"id":123,"name":format!("rvpn-{}","a".repeat(32)),"comment":format!("rvpn-{}","a".repeat(32)),"networks":[{"type":"public","ips":[{"type":"ipv4","id":"test-ip","ip":"192.0.2.1"}]}]})
}
fn deletion(responses: Vec<HttpResponse>) -> Step {
    dispatch(Call {
        method: Method::DeleteServer,
        input: json!({"operation_id":"a".repeat(32),"server_id":"123","paid_resource_ids":["test-ip"]}),
        responses,
    })
}
#[test]
fn recorded_catalog_quotes_ipv4_and_never_russia() {
    let Step::Complete { value } = dispatch(Call {
        method: Method::ListOffers,
        input: Value::Null,
        responses: catalog(),
    }) else {
        panic!()
    };
    let offers: Vec<Offer> = serde_json::from_value(value).unwrap();
    assert!(!offers.is_empty());
    assert!(offers.iter().all(Offer::eligible));
    assert_eq!(
        offers
            .iter()
            .find(|o| o.id == "6803")
            .unwrap()
            .monthly
            .decimal,
        "1820"
    );
    let mut r = catalog();
    r[2].body = json!({});
    assert!(matches!(
        dispatch(Call {
            method: Method::Quote,
            input: json!({"offer_id":"6803","location_id":"nl-1"}),
            responses: r
        }),
        Step::Error { .. }
    ));
}
#[test]
fn creation_is_one_transaction_with_no_existing_ip() {
    let Step::Complete { value } = dispatch(Call {
        method: Method::ListOffers,
        input: Value::Null,
        responses: catalog(),
    }) else {
        panic!()
    };
    let offer = value
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == "6803")
        .unwrap();
    let step = dispatch(Call {
        method: Method::CreateServer,
        input: json!({"offer":offer,"operation_id":"a".repeat(32),"cloud_init":"#cloud-config\n{}"}),
        responses: vec![
            catalog().remove(1),
            reply(json!({"servers_os":[{"name":"ubuntu","version":"24.04","id":99}]})),
        ],
    });
    let Step::Request { request } = step else {
        panic!()
    };
    assert_eq!(request.verb, "POST");
    let body = request.body.unwrap();
    assert_eq!(body["network"]["floating_ip"], "create_ip");
    assert_eq!(body["os_id"], 99);
    assert_eq!(body["availability_zone"], "ams-1");
    assert_eq!(body["is_emergency_recovery"], false);
}
#[test]
fn deletion_owns_server_and_resumes_only_receipted_ip() {
    assert!(matches!(
        deletion(vec![reply(
            json!({"server":{"id":123,"name":"production"}})
        )]),
        Step::Error { .. }
    ));
    assert!(
        matches!(deletion(vec![reply(json!({"server":owned()}))]),Step::Request{request} if request.verb=="DELETE" && request.url.ends_with("/servers/123"))
    );
    assert!(matches!(
        deletion(vec![
            absent(),
            reply(json!({"ip":{"id":"test-ip","resource_id":999}}))
        ]),
        Step::Error { .. }
    ));
    assert!(
        matches!(deletion(vec![absent(),reply(json!({"ip":{"id":"test-ip","resource_id":null}}))]),Step::Request{request} if request.verb=="DELETE" && request.url.ends_with("/floating-ips/test-ip"))
    );
    assert!(
        matches!(deletion(vec![absent(),absent()]),Step::Complete{value} if value["deleted"]==true)
    );
    assert!(
        matches!(deletion(vec![reply(json!({"server":owned()})),reply(Value::Null),reply(json!({"server":owned()}))]),Step::Error{error} if error.code=="deletion_pending")
    );
}
#[test]
fn finds_all_pages_without_duplicate_purchase() {
    let step = dispatch(Call {
        method: Method::FindServer,
        input: json!({"operation_id":"a".repeat(32)}),
        responses: vec![reply(json!({"meta":{"total":2},"servers":[owned()]}))],
    });
    assert!(
        matches!(step,Step::Request{request} if request.verb=="GET" && request.url.ends_with("offset=1"))
    );
}
#[test] fn capacity_rejection_is_explicit(){
 assert!(matches!(dispatch(Call{method:Method::CreateServer,input:Value::Null,responses:vec![HttpResponse{status:409,body:json!({"error_code":"no_free_node"})}]}),Step::Error{error} if error.code=="capacity_unavailable"));
}
