# Synthetic updater fixture

`package.txt` contains inert test bytes, signed for version `1.1.0` with the
official Tauri CLI signer. `public-key.txt` and `package.txt.sig` are public test
material. The private test key is not committed. This fixture is not an
application archive and must never be installed.

Manifest checks use the real Tauri updater against a test-only loopback service.
They assert that discovery never requests the artifact. Checking metadata does
not establish package verification or a working platform installation.
