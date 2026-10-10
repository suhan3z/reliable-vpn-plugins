# Reliable VPN provider plugins (experimental ABI v1)

Public source and signed release packages for `suhan3z/reliable-vpn-plugins`.
These are provisioning plugins, separate from the VPN packet transformation ABI.

Russian routing exceptions are maintained in [routing-lists](routing-lists/README.md), with independent categories, domain zones and a signed `routing-catalog.json` release asset. They do not change the provider ABI or provider catalog. Every latest release includes both catalogs; `publish.py` carries the current signed routing catalog unchanged.

| Provider | Current implementation |
| --- | --- |
| DigitalOcean | Public-client OAuth/PKCE descriptor; account/balance; NL/DE offers and quotes; one Droplet creation; operation-tag reconciliation; owned resource deletion |
| Timeweb Cloud | Token onboarding; live VPS + IPv4 quotes; NL/DE cloud-init creation in one POST; operation-name/comment reconciliation; deletion of the receipted VPS and IPv4. Requires client 0.4.4+. |
| Example | Third provider demonstrating installation without rebuilding the client. No network or purchase operations. |

This repository does not contain hosting credentials, device keys, or the signing private key.
Existing-account testing does not validate affiliate attribution. No affiliate URL is invented or applied to existing accounts.

## Build and test

```sh
rustup target add wasm32-unknown-unknown
cargo test --workspace --locked
cargo build --release --target wasm32-unknown-unknown --locked
```

The SDK exports `memory`, `provider_alloc(u32) -> u32`, and
`provider_call(u32,u32) -> u64`. Packed result: high 32 bits = pointer, low 32 bits = length.
The host starts a fresh instance for each invocation and drops it after reading the output.
There are **no imports**, including WASI. Input/output is UTF-8 JSON, at most 512 KiB.

`Call` contains a method, method input, and the ordered API responses received so far.
The guest returns a `Step`: a next HTTP request, normalized result, user action, or typed error.
Secrets are never included in `Call`; the host injects Bearer authorization.
OAuth registration, code exchange, rotation and vault storage are host operations.

The host rejects API redirects, origins outside the signed manifest, unauthenticated API
requests, mutating calls made through the read interface, and more than one POST per approved creation. Deletion may issue one DELETE per resource
ID in the durable operation receipt, verifying ownership before deletion and absence afterward.
Do not hide additional purchases inside GET or onboarding.

## Add a partner

1. Copy `example`, implement the common methods using `sdk`, add a manifest.
2. Return `unsupported` for capabilities the provider cannot safely implement.
3. Add recorded/synthetic response tests for price changes, foreign-region filtering,
   unknown create outcome, revoked access and resource ownership.
4. Add the crate to this workspace and the offline publishing script's provider list.
5. Publish signed packages and a new catalog generation. The client needs no provider-specific code.

ABI changes, new host capabilities and new OAuth redirect types require a client update.

## Signing and delivery

Install `cryptography` for Python, keep an Ed25519 seed **outside both checkouts**, and run:

```sh
python3 publish.py --key /secure/offline/provider.key \
  --wasm-dir target/wasm32-unknown-unknown/release \
  --output dist --generation 2 --tag v0.1.0-alpha.2
```

Use `--init-key` only for initial bootstrap. It creates a private file with mode 0600,
and writes the public key to `root.pub`. **Do not regenerate the production trust root.**
Applications pin `root.pub`; trust-root rotation needs an application release.

Packages use Ed25519 over `rvpn-provider-package-v1\0 || payload`.
Catalogs use `rvpn-provider-catalog-v1\0 || payload`; `payload` is the exact decoded bytes,
not a reserialized JSON object. Both envelopes contain base64 payload and signature.
Catalog generation must increase on every change, including revocation. Never replace
assets of an already published version. Add revoked `provider@version` strings to the catalog.

Release assets: `catalog.json`, `<id>-<version>.rvp`. The client fetches
`releases/latest/download/catalog.json` at most every six hours automatically and uses
verified cached packages while offline. No GitHub token is needed by users.
The example provider is delivered through the catalog and is deliberately not bundled.

The optional `gateway.json` descriptor is signed using `rvpn-gateway-v1\0`; it contains
`url`, `sha256`, `architecture: linux-x86_64`. The application verifies this signature
before generating cloud-init. The VPS verifies the trusted binary SHA-256 before execution.

## Sources checked during implementation

- [DigitalOcean native OAuth and PKCE](https://docs.digitalocean.com/reference/api/oauth/)
- [Timeweb API](https://timeweb.cloud/tools/api-docs)
- [Timeweb cloud-init networking requirement](https://github.com/timeweb-cloud/terraform-provider-timeweb-cloud/blob/main/docs/resources/server.md)

DigitalOcean and Timeweb paid pilots passed on 2026-10-09: signed cloud-init, pinned TLS/SSH,
DNS/HTTPS/egress through a real tunnel, reboot, and owned-resource cleanup. Timeweb also passed
portable encrypted recovery to a fresh vault. All pilot resources were deleted; pre-existing
resources were preserved. Frankfurt returned `409 no_free_node`; this is handled as a definite
capacity rejection, without an automatic purchase in a different location.

The Timeweb API payload uses the first-party web console's `network.floating_ip: "create_ip"`
transaction; this is verified live but is not fully described in the public OpenAPI schema.
Fixtures contain only public catalog responses. Full Windows UI/OS route acceptance, Android
hardware, minimum-VPS load, Russian home/mobile connectivity and affiliate attribution remain
separate release gates. Short QUIC probes showed loss; other transports passed.
