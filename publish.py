#!/usr/bin/env python3
"""Offline release signing. pip install cryptography. Private key must be outside checkout."""
import argparse, base64, hashlib, json, os, pathlib, time
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PrivateFormat, PublicFormat, NoEncryption

ROOT = pathlib.Path(__file__).resolve().parent
def encode(v): return base64.b64encode(v).decode("ascii")
def payload(v): return json.dumps(v, separators=(",", ":"), ensure_ascii=False).encode()
def signed(key, domain, value):
    data = payload(value)
    return payload({"payload": encode(data), "signature": encode(key.sign(domain + data))})
def main():
    p = argparse.ArgumentParser()
    p.add_argument("--key", type=pathlib.Path, required=True)
    p.add_argument("--wasm-dir", type=pathlib.Path, required=True)
    p.add_argument("--output", type=pathlib.Path, required=True)
    p.add_argument("--generation", type=int, required=True)
    p.add_argument("--tag", required=True)
    p.add_argument("--init-key", action="store_true")
    p.add_argument("--gateway", type=pathlib.Path)
    p.add_argument("--routing-catalog", type=pathlib.Path, default=ROOT / "routing-lists" / "catalog.signed.json")
    args = p.parse_args()
    key_path = args.key.resolve()
    if key_path.is_relative_to(ROOT.parent): p.error("Signing key must be outside application and plugin repositories")
    if args.init_key:
        key_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        key = Ed25519PrivateKey.generate()
        fd = os.open(key_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "wb") as out: out.write(key.private_bytes(Encoding.Raw, PrivateFormat.Raw, NoEncryption()))
        public = encode(key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw))
        (ROOT / "root.pub").write_text(public + "\n")
    key = Ed25519PrivateKey.from_private_bytes(key_path.read_bytes())
    assert encode(key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)) == (ROOT / "root.pub").read_text().strip(), "Signing key differs from pinned trust root"
    args.output.mkdir(parents=True, exist_ok=True)
    entries = []
    for name in ["digitalocean", "timeweb", "example"]:
        manifest = json.loads((ROOT / name / "manifest.json").read_text())
        wasm = (args.wasm_dir / f"rvpn_provider_{name}.wasm").read_bytes()
        data = signed(key, b"rvpn-provider-package-v1\0", {"manifest": manifest, "wasm": encode(wasm)})
        filename = f"{name}-{manifest['version']}.rvp"
        (args.output / filename).write_bytes(data)
        entries.append({"id": name, "version": manifest["version"], "sha256": hashlib.sha256(data).hexdigest(), "url": f"https://github.com/suhan3z/reliable-vpn-plugins/releases/download/{args.tag}/{filename}"})
    catalog = {"generation": args.generation, "expires_at": int(time.time()) + 180 * 86400, "packages": entries, "revoked": []}
    (args.output / "catalog.json").write_bytes(signed(key, b"rvpn-provider-catalog-v1\0", catalog))
    # Every latest release must retain both catalogs. Copy the already signed routing
    # artifact unchanged; refreshing its expiry requires a new routing generation.
    routing_bytes = args.routing_catalog.read_bytes()
    routing_envelope = json.loads(routing_bytes)
    routing_payload = base64.b64decode(routing_envelope["payload"], validate=True)
    key.public_key().verify(base64.b64decode(routing_envelope["signature"], validate=True), b"rvpn-routing-catalog-v1\0" + routing_payload)
    assert json.loads(routing_payload)["expires_at"] > int(time.time()), "Renew routing catalog with a new generation before publication"
    (args.output / "routing-catalog.json").write_bytes(routing_bytes)
    if args.gateway:
        import shutil
        binary = args.gateway.read_bytes()
        assert binary.startswith(b"\x7fELF"), "Gateway must be a Linux executable"
        filename = "rvpn-gateway-linux-x86_64"
        shutil.copyfile(args.gateway, args.output / filename)
        descriptor = {"url": f"https://github.com/suhan3z/reliable-vpn-plugins/releases/download/{args.tag}/{filename}", "sha256": hashlib.sha256(binary).hexdigest(), "architecture": "linux-x86_64"}
        (args.output / "gateway.json").write_bytes(signed(key, b"rvpn-gateway-v1\0", descriptor))
    print(f"Signed {len(entries)} provider packages; catalog generation {args.generation}")
if __name__ == "__main__": main()
