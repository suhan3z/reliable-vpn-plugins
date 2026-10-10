#!/usr/bin/env python3
"""CI verification of the signed artifact against the committed sources and trust root."""
import base64, json, pathlib, time
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from validate import validate

def verify(envelope, root, source):
    assert set(envelope)=={'payload','signature'}
    payload=base64.b64decode(envelope['payload'],validate=True)
    Ed25519PublicKey.from_public_bytes(base64.b64decode(root,validate=True)).verify(
        base64.b64decode(envelope['signature'],validate=True),b'rvpn-routing-catalog-v1\0'+payload)
    catalog=validate(json.loads(payload))
    assert catalog['expires_at']>int(time.time()), 'Renew expired catalog with a new generation'
    expected=dict(source,expires_at=catalog['expires_at'])
    assert catalog==expected, 'Signed artifact differs from sources'
    return catalog

if __name__=='__main__':
    directory=pathlib.Path(__file__).resolve().parent
    catalog=verify(json.loads((directory/'catalog.signed.json').read_text()),
        (directory.parent/'root.pub').read_text().strip(),
        json.loads((directory/'catalog.source.json').read_text(encoding='utf-8')))
    print(f"Verified routing signature, sources and expiry: generation {catalog['generation']}")
