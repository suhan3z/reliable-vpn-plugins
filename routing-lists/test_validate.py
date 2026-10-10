import copy,json,pathlib,unittest
from validate import validate
from verify import verify
from cryptography.exceptions import InvalidSignature

class ValidationTests(unittest.TestCase):
    def setUp(self):
        self.directory=pathlib.Path(__file__).resolve().parent
        self.source=json.loads((self.directory/'catalog.source.json').read_text(encoding='utf-8'))
    def test_format_domains_categories_and_sources(self):
        for key,value in [('domain','ru'),('domain','xn--a.ru'),('domain','EXAMPLE.ru'),('category','missing'),('source','http://example.ru'),('checked_at','2026-02-30')]:
            c=copy.deepcopy(self.source);c['entries'][0][key]=value
            with self.subTest(key=key,value=value),self.assertRaises((AssertionError,UnicodeError,ValueError)):validate(c)
        c=copy.deepcopy(self.source);c['entries'].append(c['entries'][0])
        with self.assertRaises(AssertionError):validate(c)
    def test_signed_artifact_and_tampering(self):
        envelope=json.loads((self.directory/'catalog.signed.json').read_text())
        root=(self.directory.parent/'root.pub').read_text().strip()
        verify(envelope,root,self.source)
        envelope['signature']='A'+envelope['signature'][1:]
        with self.assertRaises((InvalidSignature,ValueError)):verify(envelope,root,self.source)

if __name__=='__main__':unittest.main()
