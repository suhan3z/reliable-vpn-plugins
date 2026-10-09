//! Provider ABI v1. Guests are pure JSON -> JSON functions; the host owns I/O and secrets.
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const ABI: u32 = 1;
pub const MAX_MESSAGE: usize = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Describe,
    BeginOnboarding,
    ResumeOnboarding,
    ListLocations,
    ListOffers,
    Quote,
    BeginPayment,
    PaymentStatus,
    CreateServer,
    FindServer,
    GetServer,
    DeleteServer,
    AccountStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Auth {
    ApiToken,
    OAuthPkce,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub auth: Vec<Auth>,
    pub registration: bool,
    pub payment: bool,
    pub cloud_init: bool,
    pub create: bool,
    pub delete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub abi: u32,
    pub min_app_version: String,
    pub capabilities: Capabilities,
    pub api_origins: Vec<String>,
    pub browser_origins: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub id: String,
    pub country: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Money {
    pub decimal: String,
    pub currency: String,
}
impl Money {
    pub fn units(&self) -> Option<u128> {
        if self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
            || self.decimal.len() > 24
        {
            return None;
        }
        let mut parts = self.decimal.split('.');
        let whole = parts.next()?;
        let fraction = parts.next().unwrap_or("");
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 9
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || parts.next().is_some()
        {
            return None;
        }
        whole
            .parse::<u128>()
            .ok()?
            .checked_mul(1_000_000_000)?
            .checked_add(if fraction.is_empty() {
                0
            } else {
                fraction.parse::<u128>().ok()? * 10u128.pow(9 - fraction.len() as u32)
            })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: String,
    pub location: Location,
    pub vcpu: u32,
    pub ram_mb: u32,
    pub disk_gb: u32,
    pub architecture: String,
    pub ipv4: bool,
    pub monthly: Money,
    pub traffic: String,
    pub extra_costs: Vec<String>,
}
impl Offer {
    pub fn eligible(&self) -> bool {
        matches!(self.location.country.as_str(), "NL" | "DE")
            && self.vcpu >= 1
            && self.ram_mb >= 1024
            && self.disk_gb >= 10
            && self.architecture == "x86_64"
            && self.ipv4
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub offer: Offer,
    pub hourly: Option<Money>,
    pub total_monthly: Money,
    pub upfront: Money,
    pub expires_at: u64,
    pub complete: bool,
    pub terms: Vec<String>,
}
impl Quote {
    pub fn valid_cost(&self) -> bool {
        let Some(total) = self.total_monthly.units() else {
            return false;
        };
        let Some(base) = self.offer.monthly.units() else {
            return false;
        };
        total >= base
            && total > 0
            && self.upfront.units().is_some()
            && self.offer.monthly.currency == self.total_monthly.currency
            && self.upfront.currency == self.total_monthly.currency
            && self
                .hourly
                .as_ref()
                .is_none_or(|v| v.units().is_some() && v.currency == self.total_monthly.currency)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentState {
    NotRequired,
    Pending,
    Paid,
    Failed,
    Unsupported,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UserAction {
    EnterToken {
        help_url: String,
    },
    Browser {
        url: String,
    },
    OAuth {
        authorize_url: String,
        token_url: String,
        registration_url: String,
        scope: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServerRef {
    pub id: String,
    pub operation_id: String,
    pub name: String,
    pub ipv4: Option<String>,
    pub status: String,
    pub paid_resource_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderError {
    pub code: String,
    pub retryable: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpRequest {
    pub verb: String,
    pub url: String,
    pub body: Option<Value>,
    pub auth: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub method: Method,
    pub input: Value,
    pub responses: Vec<HttpResponse>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Request { request: HttpRequest },
    Complete { value: Value },
    Action { action: UserAction },
    Error { error: ProviderError },
}
pub fn complete(value: impl Serialize) -> Step {
    Step::Complete {
        value: serde_json::to_value(value).unwrap(),
    }
}
pub fn error(code: &str) -> Step {
    Step::Error {
        error: ProviderError {
            code: code.into(),
            retryable: false,
        },
    }
}
pub fn request(verb: &str, url: String, body: Option<Value>) -> Step {
    Step::Request {
        request: HttpRequest {
            verb: verb.into(),
            url,
            body,
            auth: true,
        },
    }
}
pub fn response_error(call: &Call) -> Option<Step> {
    call.responses
        .last()
        .filter(|r| !(200..300).contains(&r.status))
        .map(|r| Step::Error {
            error: ProviderError {
                code: match r.status {
                    401 => "access_revoked",
                    402 => "insufficient_balance",
                    403 => "permission_denied",
                    404 => "not_found",
                    429 => "rate_limited",
                    _ => "provider_failure",
                }
                .into(),
                retryable: r.status == 429 || r.status >= 500,
            },
        })
}

/// No imports or host pointers. Each call runs in a fresh bounded instance.
/// provider_call returns (output pointer << 32) | output length. Host drops instance.
#[macro_export]
macro_rules! export_provider {
    ($dispatch:path) => {
        #[no_mangle]
        pub extern "C" fn provider_alloc(len: u32) -> u32 {
            if len as usize > $crate::MAX_MESSAGE {
                return 0;
            }
            let mut bytes = vec![0u8; len as usize];
            let ptr = bytes.as_mut_ptr();
            std::mem::forget(bytes);
            ptr as u32
        }
        #[no_mangle]
        pub unsafe extern "C" fn provider_call(ptr: u32, len: u32) -> u64 {
            if len as usize > $crate::MAX_MESSAGE {
                return 0;
            }
            let bytes = std::slice::from_raw_parts(ptr as *const u8, len as usize);
            let step = match serde_json::from_slice::<$crate::Call>(bytes) {
                Ok(call) => $dispatch(call),
                Err(_) => $crate::error("invalid_input"),
            };
            let mut output = serde_json::to_vec(&step).unwrap();
            let result = ((output.as_mut_ptr() as u64) << 32) | output.len() as u64;
            std::mem::forget(output);
            result
        }
    };
}
