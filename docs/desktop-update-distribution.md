# Desktop release distribution

The current operator entry point is `scripts/release-desktop.py`. Follow the
[Chinese release runbook](client-update-local-release.zh-CN.md) for configuration,
first packages, subsequent releases, retries, metadata repair and cleanup.

## Module ownership

| File | Responsibility |
| --- | --- |
| `scripts/release-desktop.py` | Public CLI: doctor / build / stage / release / publish / check / metadata |
| `scripts/desktop_release_managed.py` | Current dual-target build orchestration, nine public objects, OSS writes and readback |
| `scripts/release-assets/download.html` | Static OS download entry, rendered with the configured public origin |
| `scripts/build-macos-clients.sh`, `macos-local-signing.py` | Existing native Mac recipe, persistent platform identity and DMG |
| `scripts/build-windows-clients.ps1` | Optional native Windows + Authenticode recipe |
| `scripts/desktop-update-release.py`, `desktop_update_local.py` | Low-level preparation/signature verification and strict native candidate/acceptance tooling |
| `src-tauri/examples/verify-update-artifacts.rs` | Official updater 2.14.1 artifact + static manifest parser protocol; no installation |

The public website uses fixed unversioned installer URLs. Both website aliases and
archived installers use versioned browser-save filenames via Content-Disposition,
such as `TokenMarket-1.1.0-mac-arm64.dmg`, generated from the release plan's version.
The application reads
`desktop-updates/production/stable.json`, whose URLs name immutable packages under
`releases/VERSION/`. Earlier OSS hash paths remain compatibility links. Do not move
an online directory merely to shorten an application-internal URL.

## Publication invariants

- Read private configuration as UTF-8 JSON outside Git; never source/eval it.
- Source must be clean and both platform records must share the full commit and version.
- Current Mac builds use the persistent identity. Windows NSIS may be cross-built on Mac;
  missing Authenticode and device validation are recorded honestly, never inferred from updater signatures.
- Verify exact final bytes and signed version with the pinned official updater before staging or writing.
- Fixture plans require an isolated loopback store; production plans and fixtures cannot be mixed.
- Permit exactly the nine planned public keys. Reject escapes, altered bytes, wrong public keys or wrong URLs.
- Serialize publication by bucket-root aliases and the existing manifest-prefix lock on the designated host/account.
- Stop on malformed/unreadable current stable metadata or an older proposed version.
- Never overwrite a version object whose bytes differ. Immutable objects are verified before fixed aliases.
- Set ACL, MIME, filename and cache headers in the same OSS command; do not depend on console upload defaults.
- Publish stable last, after every version package and website object has passed anonymous GET verification.
- Readback failure after stable PUT explicitly reports that discovery may already have advanced.

The verifier's default stdin protocol is `{version, public_key, objects, required_targets}`.
An additive `{mode:"manifest", manifest, public_key}` mode checks both platform
static targets at the current and a lower version. It does not establish Windows
execution or perform a real upgrade.

## Verification

```sh
cargo build --locked --manifest-path src-tauri/Cargo.toml --example verify-update-artifacts
python3 -m unittest discover -s tests -p 'test_update_*.py'
node --test tests/desktop-download-entry.test.cjs
```

Use `TOKEN_MARKET_TEST_BROWSER_ENGINE=webkit` for the Safari-engine download check.
Both browser engines exercise a real local HTTP attachment response, because
WebKit route mocks do not faithfully model download events.

Current managed CLI tests cover publication order, corruption stopping discovery,
immutable collisions, downgrade refusal, metadata-only repair, source/version
binding, public-path boundaries, fixture classification and suppressed child secrets.
The older preparation/acceptance fixtures remain useful and never write production OSS.
