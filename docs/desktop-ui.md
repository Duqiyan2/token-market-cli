# Lightweight desktop UI

The desktop frontend in `src/tauri-ui` now uses the approved lightweight desktop design. It contains tool cards, [device tool usage](desktop-usage.md), account/workspace management, connection details, browser-login states and help. `shell.js` handles navigation, `app.js` retains desktop-login orchestration, and the existing workspace/tool/test modules invoke the native commands. Inventory uses redacted native summaries; model names come from the saved native catalogue.

Agent details show configured models, connection ownership, test results and usage guidance. Configuration contents and the former protocol-only access-information block are omitted. Credentials and restoration receipts remain on the native side.

Page-level feedback must never shift application content when it appears, wraps or clears. `notifications.css` keeps feedback outside document flow; `notifications.js` stacks visible messages within the viewport while retaining each live region in its owning view. Settings retry actions belong to the same floating notice. Opening a tool is silent on success; launch failures retain their error dialog.

New connections and disconnections require explicit confirmation. Paid tests retain their separate fee confirmation and multi-model selection. Existing native receipt IDs, owner/workspace guards, configuration-conflict choices, permission-gated account links are preserved. The frontend contains no demo state controls or mock IPC.

## UI regression checks

With Node and Playwright available:

```sh
node --test tests/desktop-ui.test.cjs tests/desktop-usage.test.cjs tests/desktop-notifications.test.cjs
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

## Window lifecycle

On macOS and Windows, clicking the window close button hides the main window and
keeps the desktop application running. The normal minimize button retains its
minimize behavior.

On macOS, the Dock icon or menu bar icon restores the main window. On Windows,
the system tray icon restores it. Opening the application again also restores
the existing window. Restoration shows and focuses the window, including when
it was previously minimized.

An explicit Quit action ends the application. macOS also supports Command-Q;
Windows exposes Quit in the tray menu. Quitting the desktop does not disconnect
existing Tool Connections or revoke their Agent API Keys.

The first close shows an explanation before hiding. Confirming it records that
the explanation has been seen; cancelling keeps the window open. If preferences
cannot be saved, the explanation remains visible and the window stays open.

Application Settings is available before and after login, and from the tray
menu. "关闭窗口后在后台运行" defaults to enabled and can be disabled to restore
normal window-close exit behavior. It and the first-close acknowledgement are
device preferences stored in `~/.tokenmarket/desktop-preferences.json`,
independently of Client Session and account changes.

If the tray cannot be initialized, closing exits normally and Application
Settings explains that background operation is unavailable. Explicit Quit and
system exit events are never intercepted. Linux retains its existing close
behavior.

Native regression checks: `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked`.
DOM/IPC fixtures cover settings, the first-close explanation, cancellation,
save failures and tray unavailability. Actual macOS Dock/menu bar and Windows
tray restoration, Command-Q, and OS shutdown require native verification.
