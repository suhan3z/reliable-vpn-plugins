#!/usr/bin/env python3
"""Sign a separate routing catalog; never modifies provider catalog assets."""
import argparse, base64, json, pathlib, time
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
from validate import validate
p=argparse.ArgumentParser();p.add_argument('--key',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--generation',type=int,required=True)
a=p.parse_args();root=pathlib.Path(__file__).resolve().parent
assert not a.key.resolve().is_relative_to(root.parent.parent), 'Private key must stay outside application and plugin checkouts'
key=Ed25519PrivateKey.from_private_bytes(a.key.read_bytes())
assert base64.b64encode(key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw)).decode()==(root.parent/'root.pub').read_text().strip(), 'Pinned trust root mismatch'
c=validate(json.loads((root/'catalog.source.json').read_text(encoding='utf-8')))
assert a.generation>=c['generation'];c['generation']=a.generation;c['expires_at']=int(time.time())+180*86400
data=json.dumps(c,separators=(',',':'),ensure_ascii=False).encode()
envelope={'payload':base64.b64encode(data).decode(),'signature':base64.b64encode(key.sign(b'rvpn-routing-catalog-v1\0'+data)).decode()}
a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(envelope,separators=(',',':')),encoding='utf-8')
print(f'Signed routing catalog generation {a.generation}, {len(c["entries"])} domains')
