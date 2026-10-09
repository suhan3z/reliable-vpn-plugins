use rvpn_provider_sdk::*;
use serde_json::{json, Value};
const API: &str = "https://api.digitalocean.com/v2";
pub fn manifest() -> Manifest {
    serde_json::from_str(include_str!("../manifest.json")).unwrap()
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn location(id: &str) -> Option<Location> {
    let (country, name) = match id {
        "ams3" => ("NL", "Amsterdam"),
        "fra1" => ("DE", "Frankfurt"),
        _ => return None,
    };
    Some(Location {
        id: id.into(),
        country: country.into(),
        name: name.into(),
    })
}
fn money(v: &Value) -> Money {
    Money {
        decimal: v.to_string().trim_matches('"').into(),
        currency: "USD".into(),
    }
}
fn server(v: &Value, operation: &str) -> Option<ServerRef> {
    let name = format!("rvpn-{operation}");
    if s(v, "name") != name
        || !v["tags"]
            .as_array()?
            .iter()
            .any(|t| t.as_str() == Some(&name))
    {
        return None;
    }
    let ipv4 = v["networks"]["v4"]
        .as_array()
        .and_then(|a| a.iter().find(|n| s(n, "type") == "public"))
        .and_then(|n| n["ip_address"].as_str())
        .map(String::from);
    Some(ServerRef {
        id: v["id"].as_u64()?.to_string(),
        operation_id: operation.into(),
        name,
        ipv4,
        status: s(v, "status").into(),
        paid_resource_ids: vec![],
    })
}
pub fn dispatch(call: Call) -> Step {
    if call.method == Method::DeleteServer && call.responses.last().is_some_and(|r| r.status == 404)
    {
        return complete(json!({"deleted":true}));
    }
    if let Some(e) = response_error(&call) {
        return e;
    }
    let input = &call.input;
    let responses = &call.responses;
    let n = responses.len();
    match call.method {
        Method::Describe=>complete(manifest()),
        Method::BeginOnboarding=>Step::Action{action:UserAction::OAuth{authorize_url:"https://cloud.digitalocean.com/v1/oauth/authorize".into(),token_url:"https://cloud.digitalocean.com/v1/oauth/token".into(),registration_url:"https://cloud.digitalocean.com/v1/oauth/register".into(),scope:"account:read billing:read droplet:read droplet:create droplet:delete regions:read sizes:read image:read tag:read tag:create".into()}},
        Method::ResumeOnboarding=>complete(json!({"requires_account_check":true})),
        Method::BeginPayment=>Step::Action{action:UserAction::Browser{url:"https://cloud.digitalocean.com/account/billing".into()}},
        Method::PaymentStatus=>complete(PaymentState::Unsupported),
        Method::AccountStatus=>{
            if n==0{return request("GET",format!("{API}/account"),None)}
            if n==1{return request("GET",format!("{API}/customers/my/balance"),None)}
            complete(json!({"active":responses[0].body["account"]["status"]=="active","server_limit":responses[0].body["account"]["droplet_limit"],"balance":responses[1].body["account_balance"],"currency":"USD"}))
        },
        Method::ListLocations=>{
            if n==0{return request("GET",format!("{API}/regions?per_page=200"),None)}
            if responses[0].body["links"]["pages"]["next"].is_string(){return error("pagination_required")}
            let values:Vec<_>=responses[0].body["regions"].as_array().into_iter().flatten().filter(|r|r["available"]==true).filter_map(|r|location(s(r,"slug"))).collect();complete(values)
        },
        Method::ListOffers|Method::Quote=>{
            if n==0{return request("GET",format!("{API}/sizes?per_page=200"),None)}
            if responses[0].body["links"]["pages"]["next"].is_string(){return error("pagination_required")}
            let mut offers=vec![];
            for size in responses[0].body["sizes"].as_array().into_iter().flatten(){
                // General-purpose x86 basic droplets only; no guessed architecture for new families.
                if size["available"]!=true || !s(size,"slug").starts_with("s-"){continue}
                for region in size["regions"].as_array().into_iter().flatten(){
                    if let Some(loc)=region.as_str().and_then(location){
                        let offer=Offer{id:s(size,"slug").into(),location:loc,vcpu:size["vcpus"].as_u64().unwrap_or(0) as u32,ram_mb:size["memory"].as_u64().unwrap_or(0) as u32,disk_gb:size["disk"].as_u64().unwrap_or(0) as u32,architecture:"x86_64".into(),ipv4:true,monthly:money(&size["price_monthly"]),traffic:format!("{} TB outbound/month; overage billed by provider",size["transfer"]),extra_costs:vec!["Account taxes and traffic overage are billed separately. Backups and volumes are disabled.".into()]};
                        if offer.eligible(){offers.push((offer,money(&size["price_hourly"])));}
                    }
                }
            }
            if call.method==Method::ListOffers {return complete(offers.into_iter().map(|(o,_)|o).collect::<Vec<_>>())}
            let Some((offer,hourly))=offers.into_iter().find(|(o,_)|o.id==s(input,"offer_id") && o.location.id==s(input,"location_id")) else{return error("offer_unavailable")};
            complete(Quote{total_monthly:offer.monthly.clone(),upfront:Money{decimal:"0".into(),currency:"USD".into()},offer,hourly:Some(hourly),expires_at:input["now"].as_u64().unwrap_or(0)+300,complete:true,terms:vec!["Creation starts hourly billing until deletion, including while powered off.".into(),"Monthly amount is before account taxes and usage overage; this is not a hard spending cap.".into()]})
        },
        Method::CreateServer=>{
            if n>0{return server(&responses[0].body["droplet"],s(input,"operation_id")).map(complete).unwrap_or_else(||error("creation_unresolved"))}
            let Ok(offer)=serde_json::from_value::<Offer>(input["offer"].clone()) else{return error("invalid_offer")};
            let operation=s(input,"operation_id");
            if !offer.eligible() || location(&offer.location.id).is_none() || operation.len()!=32 || !operation.bytes().all(|b|b.is_ascii_hexdigit()) {return error("invalid_operation")}
            let init=s(input,"cloud_init");if !init.starts_with("#cloud-config") || init.len()>65536{return error("invalid_cloud_init")}
            request("POST",format!("{API}/droplets"),Some(json!({"name":format!("rvpn-{operation}"),"region":offer.location.id,"size":offer.id,"image":"ubuntu-24-04-x64","backups":false,"ipv6":false,"monitoring":false,"tags":[format!("rvpn-{operation}")],"user_data":init})))
        },
        Method::FindServer=>{
            let operation=s(input,"operation_id");if operation.len()!=32 || !operation.bytes().all(|b|b.is_ascii_hexdigit()){return error("invalid_operation")}
            if n==0{return request("GET",format!("{API}/droplets?tag_name=rvpn-{operation}&per_page=200"),None)}
            if responses[0].body["links"]["pages"]["next"].is_string(){return error("ambiguous_resources")}
            complete(responses[0].body["droplets"].as_array().into_iter().flatten().filter_map(|v|server(v,operation)).collect::<Vec<_>>())
        },
        Method::GetServer|Method::DeleteServer=>{
            let id=s(input,"server_id");if id.is_empty() || !id.bytes().all(|b|b.is_ascii_digit()){return error("invalid_server")}
            if n==0{return request("GET",format!("{API}/droplets/{id}"),None)}
            let Some(owned)=server(&responses[0].body["droplet"],s(input,"operation_id")) else{return error("ownership_mismatch")};
            if call.method==Method::GetServer{return complete(owned)}
            if n==1{return request("DELETE",format!("{API}/droplets/{id}"),None)}
            complete(json!({"deleted":true}))
        }
    }
}
rvpn_provider_sdk::export_provider!(dispatch);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_russian_offer_and_no_implicit_ipv6() {
        let call = Call {
            method: Method::ListOffers,
            input: Value::Null,
            responses: vec![HttpResponse {
                status: 200,
                body: json!({"sizes":[{"slug":"s-1vcpu-1gb","available":true,"vcpus":1,"memory":1024,"disk":25,"price_monthly":6,"price_hourly":0.009,"transfer":1,"regions":["ams3","fra1","ru-1"]}]}),
            }],
        };
        let Step::Complete { value } = dispatch(call) else {
            panic!()
        };
        assert_eq!(value.as_array().unwrap().len(), 2);
    }
    #[test]
    fn cannot_delete_unowned() {
        let out = dispatch(Call {
            method: Method::DeleteServer,
            input: json!({"server_id":"123","operation_id":"a".repeat(32)}),
            responses: vec![HttpResponse {
                status: 200,
                body: json!({"droplet":{"id":123,"name":"production","tags":[]}}),
            }],
        });
        assert!(matches!(out, Step::Error { .. }));
    }
}
