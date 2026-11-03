# Client account login

The CLI and desktop share login through `tokenmarket_core::desktop::DesktopLogin`.
Login is independent of Tool Connection creation and never sends a model request.

## Browser return

`begin` creates S256 PKCE and a random state. `complete` validates the fixed
`tokenmarket://auth/callback` scheme, request identity, state and expiry before
exchanging a one-time code. `cancel` prevents late completion from becoming a
local login. Callbacks and credentials must not be printed.

Pending requests are saved in the local application data directory so callbacks
can launch a closed app. On Unix, the directory is `0700` and the file is `0600`.
The verifier is deleted on consumption, expiry or cancellation. Tauri deep-link
handling covers cold launch and an already running app; Windows secondary
launches are forwarded to the existing instance.

For SSH use, `tokenmarket login --no-browser` displays an authorization link that
can be opened on another device. Only the initiating client collects the result.

## Local session storage

Reusable sessions use private credential storage. macOS uses Keychain and Windows
uses Credential Manager. Linux storage behavior is described in
[shared credential handling](adr/0004-shared-client-key-lifecycle.md).
Credentials are scoped to the configured service address. Native status responses
expose account, workspace and expiry without exposing the token.

`CachedDesktopCredentials` reads private storage lazily and caches successful
results in Rust memory. Changed sessions are persisted before updating the cache.
Failed reads, writes and deletions are reported and are not cached as success.
A new process reads private storage again.

Startup, focus and a five-minute interval refresh the saved session. Invalid or
expired sessions return to browser login. Logout removes the local credential
and attempts remote sign-out; an unconfirmed remote result is shown explicitly.
These actions preserve Agent configuration, saved tool credentials and backups.

## Workspace and account actions

The signed-in header displays Current Workspace and balance. Workspace switching
updates the shared selection without rewriting existing Tool Connections. Balance
values remain decimal strings and retain their supplied currency.

Account actions accept only the supported profile, tokens, billing, workspaces
and recharge destinations. The client opens the authenticated session's supplied
web origin, or an explicit `TOKEN_MARKET_CONSOLE_WEB_BASE_URL` override. Missing
origin fails with a sign-in prompt. Browser URLs contain only the requested
workspace and action, never login credentials. Returning focus refreshes the
balance without assuming that a payment succeeded.

## Verification

Automated tests use temporary directories, fake HTTP and fake private stores.
They cover callback validation, cancellation, cold recovery, renewal, expiry,
logout failure, account switching and preservation of local tool configuration.
They do not establish native OS credential-store or installed-protocol behavior.

The optional private-store smoke uses a random `.invalid` namespace and fake
content, reads it from another process, then deletes only its own fixture:

```sh
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --test native_credentials --locked -- --ignored --nocapture
```

Check installed-app cold/warm callbacks, manual browser return, restart and
locked-store behavior separately on each supported operating system. Local
signing details are in [macOS local signing](macos-local-signing.md).
