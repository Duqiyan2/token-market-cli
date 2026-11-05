# Synthetic updater fixture

`package.txt` contains inert test bytes, signed for version `1.1.0` with the
official Tauri CLI signer. `public-key.txt` and `package.txt.sig` are public test
material. The private test key is not committed. This fixture is not an
application archive and must never be installed.

Manifest checks use the real Tauri updater against a test-only loopback service.
They assert that discovery never requests the artifact. Checking metadata does
not establish package verification or a working platform installation.

Confirmed-download tests use the real updater to verify these bytes with signed
version enforcement. `wrong-key.sig` signs the same bytes and version with a
separate synthetic key and must fail against `public-key.txt`. The matching wrong
private key is also outside the repository. Tests also cover altered bytes,
malformed/missing signatures, a mismatched advertised version, connection loss,
HTTP failure, retry, stale consent and checks during downloading/ready states.
No fixture operation calls install or restart. Production requests and redirects
remain HTTPS-only; loopback HTTP is confined to native test configuration.
