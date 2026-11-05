"""Temporary diagnostic for the generated CI fixture identity only.
Never invoke against a persistent/release identity or with release secrets.
"""
import argparse
import importlib.util
from pathlib import Path
import subprocess
import sys

PREFIX = '[DEBUG-ci-identity-20261105]'
identity = Path(sys.argv[1]).resolve()
expected = Path(__import__('os').environ['RUNNER_TEMP']).resolve()/'tokenmarket-signing-fixture'
if identity != expected:
    raise SystemExit('Refusing identity outside the exact disposable CI fixture path')
script = Path('scripts/macos-local-signing.py').resolve()
spec = importlib.util.spec_from_file_location('fixture_signing', script)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def report(label, command):
    result = subprocess.run([str(arg) for arg in command], capture_output=True, text=True)
    print(PREFIX, label, 'exit=' + str(result.returncode), flush=True)
    # Only explicitly scoped public certificate/identity metadata is requested.
    print(result.stdout, result.stderr, sep='', flush=True)

try:
    with module.identity(identity) as (keychain, digest):
        print(PREFIX, 'Fixture SHA1=' + digest, flush=True)
        report('public certificate', ['/usr/bin/openssl','x509','-in',identity/'certificate.pem','-noout','-subject','-issuer','-dates','-fingerprint','-sha1'])
        report('code-signing identities (all)', ['/usr/bin/security','find-identity','-p','codesigning',keychain])
        report('code-signing identities (valid)', ['/usr/bin/security','find-identity','-v','-p','codesigning',keychain])
        report('basic identities', ['/usr/bin/security','find-identity','-p','basic',keychain])
        report('fixture certificate policy', ['/usr/bin/security','verify-cert','-c',identity/'certificate.pem','-p','codeSign','-L','-k',keychain])
except Exception as error:
    print(PREFIX, 'PROBE_ERROR', str(error), flush=True)
