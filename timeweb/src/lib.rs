use rvpn_provider_sdk::*;
use serde_json::json;
const API: &str = "https://api.timeweb.cloud/api";
pub fn manifest() -> Manifest {
    serde_json::from_str(include_str!("../manifest.json")).unwrap()
}
pub fn dispatch(call: Call) -> Step {
    if let Some(e) = response_error(&call) {
        return e;
    }
    let n = call.responses.len();
    match call.method {
        Method::Describe => complete(manifest()),
        Method::BeginOnboarding => Step::Action {
            action: UserAction::EnterToken {
                help_url: "https://timeweb.cloud/my/api-keys".into(),
            },
        },
        Method::ResumeOnboarding => complete(json!({"requires_account_check":true})),
        Method::BeginPayment => Step::Action {
            action: UserAction::Browser {
                url: "https://timeweb.cloud/my/finances".into(),
            },
        },
        Method::PaymentStatus => complete(PaymentState::Unsupported),
        Method::AccountStatus => {
            if n == 0 {
                return request("GET", format!("{API}/v1/account/finances"), None);
            }
            let f = &call.responses[0].body["finances"];
            complete(json!({"balance":f["balance"],"currency":f["currency"],"active":!f.is_null()}))
        }
        Method::ListLocations => {
            if n == 0 {
                return request("GET", format!("{API}/v2/locations"), None);
            }
            complete(
                call.responses[0].body["locations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|l| {
                        let country = l["location_code"].as_str()?;
                        if !matches!(country, "NL" | "DE") {
                            return None;
                        }
                        let id = l["location"].as_str()?;
                        Some(Location {
                            id: id.into(),
                            country: country.into(),
                            name: id.into(),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        Method::ListOffers | Method::Quote => {
            if n == 0 {
                return request("GET", format!("{API}/v1/presets/servers"), None);
            }
            if n == 1 {
                return request("GET", format!("{API}/v2/locations"), None);
            }
            let mut offers = vec![];
            for p in call.responses[0].body["server_presets"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let loc = p["location"].as_str().unwrap_or("");
                let Some(country) = call.responses[1].body["locations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|l| l["location"] == loc)
                    .and_then(|l| l["location_code"].as_str())
                    .filter(|c| matches!(*c, "NL" | "DE"))
                else {
                    continue;
                };
                let offer = Offer {
                    id: p["id"].to_string(),
                    location: Location {
                        id: loc.into(),
                        country: country.into(),
                        name: loc.into(),
                    },
                    vcpu: p["cpu"].as_u64().unwrap_or(0) as u32,
                    ram_mb: p["ram"].as_u64().unwrap_or(0) as u32,
                    disk_gb: (p["disk"].as_u64().unwrap_or(0) / 1024) as u32,
                    architecture: "x86_64".into(),
                    ipv4: true,
                    monthly: Money {
                        decimal: p["price"].to_string(),
                        currency: "RUB".into(),
                    },
                    traffic: format!(
                        "{} Mbps; transfer allowance needs account verification",
                        p["bandwidth"]
                    ),
                    extra_costs: vec![
                        "Public IPv4, taxes and traffic terms need account verification.".into(),
                    ],
                };
                if offer.eligible() {
                    offers.push(offer)
                }
            }
            if call.method == Method::ListOffers {
                return complete(offers);
            }
            let Some(offer) = offers.into_iter().find(|o| {
                o.id == call.input["offer_id"] && o.location.id == call.input["location_id"]
            }) else {
                return error("offer_unavailable");
            };
            complete(Quote{total_monthly:offer.monthly.clone(),offer,hourly:None,upfront:Money{decimal:"0".into(),currency:"RUB".into()},expires_at:call.input["now"].as_u64().unwrap_or(0)+300,complete:false,terms:vec!["Purchase unavailable until floating IPv4 pricing and cloud-init network behavior are verified on the account.".into()]})
        }
        // Multi-resource purchase needs independent price/ownership receipts for the floating IP.
        // Never report success or quietly buy an unquoted additional resource.
        Method::CreateServer | Method::FindServer | Method::GetServer | Method::DeleteServer => {
            error("unsupported_pending_account_validation")
        }
    }
}
rvpn_provider_sdk::export_provider!(dispatch);
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn no_unsupported_purchase() {
        assert!(matches!(
            dispatch(Call {
                method: Method::CreateServer,
                input: Value::Null,
                responses: vec![]
            }),
            Step::Error { .. }
        ));
    }
    #[test]
    fn ru_only_is_empty() {
        let out = dispatch(Call {
            method: Method::ListOffers,
            input: Value::Null,
            responses: vec![
                HttpResponse {
                    status: 200,
                    body: json!({"server_presets":[{"id":1,"location":"ru-1","cpu":1,"ram":1024,"disk":15360,"price":200}]}),
                },
                HttpResponse {
                    status: 200,
                    body: json!({"locations":[{"location":"ru-1","location_code":"RU"}]}),
                },
            ],
        });
        assert!(matches!(out,Step::Complete{value} if value==json!([])));
    }
}
