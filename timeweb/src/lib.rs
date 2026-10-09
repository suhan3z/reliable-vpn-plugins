use rvpn_provider_sdk::*;
use serde_json::{json, Value};
const API: &str = "https://api.timeweb.cloud/api";
pub fn manifest() -> Manifest {
    serde_json::from_str(include_str!("../manifest.json")).unwrap()
}
pub fn dispatch(call: Call) -> Step {
    if call.responses.last().is_some_and(|r|r.status==409 && r.body["error_code"]=="no_free_node") {return error("capacity_unavailable");}
    if call.method == Method::DeleteServer {
        return delete(&call);
    }
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
            if n == 2 {
                return request("GET", format!("{API}/v1/additional-services/servers"), None);
            }
            let Some(ip_price) = call.responses[2].body["servers_additional_services"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["name"] == "ip_vds")
                .and_then(|p| amount(&p["price"]))
            else {
                return error("ipv4_price_unavailable");
            };
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
                        decimal: match amount(&p["price"]).and_then(|v| v.checked_add(ip_price)) { Some(v) => decimal(v), None => continue },
                        currency: "RUB".into(),
                    },
                    traffic: format!(
                        "{} Mbps; трафик без фиксированной квоты, fair use: https://timeweb.cloud/docs/cloud-servers/limitations",
                        p["bandwidth"]
                    ),
                    extra_costs: vec![
                        format!("IPv4 включён: {} RUB/мес. Резервные копии, DDoS-защита и дополнительные диски не заказываются.", decimal(ip_price)),
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
            complete(Quote{total_monthly:offer.monthly.clone(),offer,hourly:None,upfront:Money{decimal:"0".into(),currency:"RUB".into()},expires_at:call.input["now"].as_u64().unwrap_or(0)+300,complete:true,terms:vec!["Стоимость включает VPS и один публичный IPv4. Тарификация начинается при создании и продолжается до удаления обоих ресурсов.".into(), "Дополнительные услуги отключены. Применяются действующие условия трафика и налогообложения аккаунта.".into()]})
        }
        Method::CreateServer => create(&call),
        Method::FindServer => {
            let op = text(&call.input, "operation_id");
            if !operation(op) {
                return error("invalid_operation");
            }
            if n == 0 {
                return request("GET", format!("{API}/v1/servers?limit=100&offset=0"), None);
            }
            let seen: usize = call
                .responses
                .iter()
                .map(|r| r.body["servers"].as_array().map_or(0, Vec::len))
                .sum();
            let total = call.responses.last().unwrap().body["meta"]["total"]
                .as_u64()
                .unwrap_or(u64::MAX);
            if total > seen as u64 {
                if n >= 20
                    || call.responses.last().unwrap().body["servers"]
                        .as_array()
                        .is_none_or(Vec::is_empty)
                {
                    return error("ambiguous_resources");
                }
                return request(
                    "GET",
                    format!("{API}/v1/servers?limit=100&offset={seen}"),
                    None,
                );
            }
            complete(
                call.responses
                    .iter()
                    .flat_map(|r| r.body["servers"].as_array().into_iter().flatten())
                    .filter_map(|v| server(v, op))
                    .collect::<Vec<_>>(),
            )
        }
        Method::GetServer => {
            let id = text(&call.input, "server_id");
            if !identifier(id) {
                return error("invalid_server");
            }
            if n == 0 {
                return request("GET", format!("{API}/v1/servers/{id}"), None);
            }
            server(
                &call.responses[0].body["server"],
                text(&call.input, "operation_id"),
            )
            .filter(|s| s.id == id)
            .map(complete)
            .unwrap_or_else(|| error("ownership_mismatch"))
        }
        Method::DeleteServer => unreachable!(),
    }
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn operation(v: &str) -> bool {
    v.len() == 32 && v.bytes().all(|c| c.is_ascii_hexdigit())
}
fn identifier(v: &str) -> bool {
    !v.is_empty() && v.len() <= 64 && v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn amount(v: &Value) -> Option<u128> {
    Money {
        decimal: v.to_string().trim_matches('"').into(),
        currency: "RUB".into(),
    }
    .units()
}
fn decimal(v: u128) -> String {
    let v = format!("{}.{:09}", v / 1_000_000_000, v % 1_000_000_000);
    v.trim_end_matches('0').trim_end_matches('.').into()
}
fn server(v: &Value, op: &str) -> Option<ServerRef> {
    let name = format!("rvpn-{op}");
    if !operation(op) || v["name"] != name || v["comment"] != name {
        return None;
    }
    let ips: Vec<_> = v["networks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|n| n["type"] == "public")
        .flat_map(|n| n["ips"].as_array().into_iter().flatten())
        .filter(|ip| ip["type"] == "ipv4")
        .collect();
    if ips.len() > 1 {
        return None;
    }
    Some(ServerRef {
        id: v["id"].as_u64()?.to_string(),
        operation_id: op.into(),
        name,
        ipv4: ips
            .first()
            .and_then(|ip| ip["ip"].as_str())
            .map(String::from),
        status: text(v, "status").into(),
        paid_resource_ids: ips
            .iter()
            .filter_map(|ip| {
                ip["id"]
                    .as_str()
                    .filter(|id| identifier(id))
                    .map(String::from)
            })
            .collect(),
    })
}
fn create(call: &Call) -> Step {
    let n = call.responses.len();
    let i = &call.input;
    let op = text(i, "operation_id");
    let Ok(offer) = serde_json::from_value::<Offer>(i["offer"].clone()) else {
        return error("invalid_offer");
    };
    if !operation(op) || !offer.eligible() || !offer.id.bytes().all(|c| c.is_ascii_digit()) {
        return error("invalid_operation");
    }
    if n == 0 {
        return request("GET", format!("{API}/v2/locations"), None);
    }
    if n == 1 {
        return request("GET", format!("{API}/v1/os/servers"), None);
    }
    let Some(zone) = call.responses[0].body["locations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| {
            l["location"] == offer.location.id && l["location_code"] == offer.location.country
        })
        .and_then(|l| l["availability_zones"].as_array())
        .and_then(|z| z.first())
        .and_then(Value::as_str)
    else {
        return error("location_unavailable");
    };
    let Some(os) = call.responses[1].body["servers_os"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| text(o, "name").eq_ignore_ascii_case("ubuntu") && o["version"] == "24.04")
        .and_then(|o| o["id"].as_u64())
    else {
        return error("os_unavailable");
    };
    if n == 2 {
        let init = text(i, "cloud_init");
        if !init.starts_with("#cloud-config") || init.len() > 65536 {
            return error("invalid_cloud_init");
        }
        return request(
            "POST",
            format!("{API}/v1/servers"),
            Some(
                json!({"name":format!("rvpn-{op}"),"comment":format!("rvpn-{op}"),"hostname":format!("rvpn-{op}"),"preset_id":offer.id.parse::<u64>().unwrap_or(0),"os_id":os,"availability_zone":zone,"cloud_init":init,"is_root_password_required":false,"is_ddos_guard":false,"is_emergency_recovery":false,"is_soft_remove_recovery":false,"network":{"floating_ip":"create_ip","network_drive_ids":[]}}),
            ),
        );
    }
    server(&call.responses[2].body["server"], op)
        .map(complete)
        .unwrap_or_else(|| error("creation_unresolved"))
}
fn delete(call: &Call) -> Step {
    let i = &call.input;
    let id = text(i, "server_id");
    let op = text(i, "operation_id");
    let n = call.responses.len();
    if !identifier(id) || !operation(op) {
        return error("invalid_operation");
    }
    if call
        .responses
        .last()
        .is_some_and(|r| r.status != 404 && !(200..300).contains(&r.status))
    {
        return response_error(call).unwrap();
    }
    let Some(ids) = i["paid_resource_ids"].as_array() else {
        return error("resource_receipt_missing");
    };
    // This plugin only ever purchases one IPv4. Do not infer ownership from a free IP list.
    if ids.len() != 1 || !ids[0].as_str().is_some_and(identifier) {
        return error("resource_receipt_missing");
    }
    let ip = ids[0].as_str().unwrap();
    if n == 0 {
        return request("GET", format!("{API}/v1/servers/{id}"), None);
    }
    let mut cursor = 1;
    if call.responses[0].status != 404 {
        let Some(owned) = server(&call.responses[0].body["server"], op).filter(|s| s.id == id)
        else {
            return error("ownership_mismatch");
        };
        if owned.paid_resource_ids != vec![ip.to_string()] {
            return error("ownership_mismatch");
        }
        if n == cursor {
            return request("DELETE", format!("{API}/v1/servers/{id}"), None);
        }
        cursor += 1;
        if n == cursor {
            return request("GET", format!("{API}/v1/servers/{id}"), None);
        }
        if call.responses[cursor].status != 404 {
            return error("deletion_pending");
        }
        cursor += 1;
    }
    if n == cursor {
        return request("GET", format!("{API}/v1/floating-ips/{ip}"), None);
    }
    let r = &call.responses[cursor];
    cursor += 1;
    if r.status == 404 {
        return complete(json!({"deleted":true}));
    }
    let v = &r.body["ip"];
    // An address moved to any resource after purchase belongs to that resource now.
    if v["id"] != ip || !v["resource_id"].is_null() {
        return error("resource_still_attached");
    }
    if n == cursor {
        return request("DELETE", format!("{API}/v1/floating-ips/{ip}"), None);
    }
    cursor += 1;
    if n == cursor {
        return request("GET", format!("{API}/v1/floating-ips/{ip}"), None);
    }
    if call.responses[cursor].status != 404 {
        return error("deletion_pending");
    }
    complete(json!({"deleted":true}))
}
rvpn_provider_sdk::export_provider!(dispatch);
#[cfg(test)]
mod tests;
