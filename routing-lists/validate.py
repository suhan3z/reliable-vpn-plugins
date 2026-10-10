#!/usr/bin/env python3
"""Validate data-only sources without network or publishing credentials."""
import datetime, ipaddress, json, pathlib, re, sys, urllib.parse
def validate(c):
    assert set(c)=={'version','generation','expires_at','updated_at','categories','zones','entries'}
    assert c['version']==1 and isinstance(c['generation'],int) and c['generation']>0
    datetime.date.fromisoformat(c['updated_at'])
    ids=[v['id'] for v in c['categories']]
    assert len(ids)==len(set(ids)) and {'banks','government'}<=set(ids)
    assert len(ids)<=16 and all(re.fullmatch('[a-z-]+',i) for i in ids)
    assert all(not v['default_enabled'] or v['id'] in ('banks','government') for v in c['categories'])
    assert sorted(c['zones'])==sorted(['ru','xn--p1ai','su'])
    seen=set()
    assert len(c['entries'])<=8192
    for e in c['entries']:
        assert set(e)=={'domain','subdomains','service','category','source','reason','checked_at'}
        d=e['domain']; assert '.' in d and len(d)<=253 and d not in seen
        assert d.encode('idna').decode('ascii').lower()==d
        assert d.encode('ascii').decode('idna').encode('idna').decode('ascii')==d
        assert all(re.fullmatch(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?',p) for p in d.split('.'))
        try: ipaddress.ip_address(d)
        except ValueError: pass
        else: raise AssertionError('IP is not a domain')
        assert e['category'] in ids and isinstance(e['subdomains'],bool)
        assert e['service'] and e['reason']
        source=urllib.parse.urlsplit(e['source']); assert source.scheme=='https' and source.hostname and not source.username
        datetime.date.fromisoformat(e['checked_at']);seen.add(d)
    return c
if __name__=='__main__':
    p=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else pathlib.Path(__file__).with_name('catalog.source.json')
    c=validate(json.loads(p.read_text(encoding='utf-8')))
    print(f"Routing lists: {len(c['categories'])} categories, {len(c['entries'])} domains, 3 independent zones")
