# Lightweight desktop UI

The desktop frontend in `src/tauri-ui` now uses the approved lightweight desktop design. It contains tool cards, account/workspace management, connection details, browser-login states and help. `shell.js` handles navigation, `app.js` retains desktop-login orchestration, and the existing workspace/tool/test modules invoke the native commands. Inventory uses redacted native summaries; model names come from the saved native catalogue.

Agent details show configured models, connection ownership, test results and usage guidance. Configuration contents and the former protocol-only access-information block are omitted. Credentials and restoration receipts remain on the native side.

New connections and disconnections require explicit confirmation. Paid tests retain their separate fee confirmation and multi-model selection. Existing native receipt IDs, owner/workspace guards, configuration-conflict choices, permission-gated account links are preserved. The frontend contains no demo state controls or mock IPC.

## UI regression checks

With Node and Playwright available:

```sh
node --test tests/desktop-ui.test.cjs
```

Set `TOKEN_MARKET_TEST_BROWSER_CHANNEL=chrome` to use an installed Google Chrome. If Playwright is provided by an external dependency runtime, point `NODE_PATH` at its node_modules directory. Tests use a fresh browser context and an isolated native-IPC fixture. They do not access real credentials, edit Agent configuration, launch tools or send model requests.

The UI checks cover navigation/search, fresh connection confirmation, actual catalogue model names, paid testing with saved receipt IDs, current versus billed workspace, reconnect ownership guards, disconnect conflicts, logout with late results, permissions, unavailable balances and pending confirmation invalidation.

## Native bundle

Run `scripts/build-local-desktop.sh --bundles app -- --offline --locked` to build
an application from the same frontend assets. `TAURI_CLI_JS` can point to an
installed Tauri JS CLI when `cargo tauri` is unavailable. For paired CLI/desktop
signing, see [macOS local signing](macos-local-signing.md).

Packaging verifies compilation. Installed login, browser return, OS credential
access and model requests require separate native verification. Browser fixture
screenshots and visual checks are in `design-demos/`.
