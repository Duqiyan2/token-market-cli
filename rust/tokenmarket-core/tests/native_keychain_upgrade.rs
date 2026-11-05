//! Driven only by macos-local-signing.py native-smoke with signed test processes.
//! Uses the production credential implementation and unique fake scopes.
#![cfg(target_os = "macos")]

use anyhow::{bail, ensure, Context, Result};
use tokenmarket_core::desktop::{DesktopCredentialStore, NativeDesktopCredentials};

#[link(name = "Security", kind = "framework")]
extern "C" {
    fn SecKeychainSetUserInteractionAllowed(allowed: u8) -> i32;
}

#[test]
#[ignore = "signed-process probe; writes only a unique fake .invalid credential"]
fn native_keychain_upgrade_probe() -> Result<()> {
    // Referencing this build-time value forces distinct old/new executables.
    let variant = option_env!("TOKEN_MARKET_KEYCHAIN_PROBE_BUILD").unwrap_or("default");
    let scope = std::env::var("TOKEN_MARKET_KEYCHAIN_PROBE_SCOPE")
        .context("run this probe through macos-local-signing.py native-smoke")?;
    let nonce = scope
        .strip_prefix("https://native-keychain-upgrade-")
        .and_then(|value| value.strip_suffix(".invalid"))
        .context("probe requires an isolated fake credential scope")?;
    ensure!(
        nonce.len() == 32 && nonce.bytes().all(|value| value.is_ascii_hexdigit()),
        "invalid probe scope"
    );
    // A required authorization must fail the test instead of displaying a dialog.
    ensure!(
        unsafe { SecKeychainSetUserInteractionAllowed(0) } == 0,
        "cannot disable keychain UI"
    );
    let store = NativeDesktopCredentials::new(&scope);
    let initial = format!("fake-keychain-upgrade-initial:{nonce}");
    let updated = format!("fake-keychain-upgrade-updated:{nonce}");
    use sha2::{Digest, Sha256};
    let native = keyring::Entry::new(
        "cn.tokenmarket.setup.desktop-session",
        &format!("platform-{:x}", Sha256::digest(scope.as_bytes())),
    )?;
    let operation = std::env::var("TOKEN_MARKET_KEYCHAIN_PROBE_OPERATION")?;
    match operation.as_str() {
        "missing" => ensure!(store.read()?.is_none(), "fixture already exists"),
        "create" => {
            ensure!(store.read()?.is_none(), "fixture already exists");
            store.write(&initial)?;
        }
        "read" => ensure!(
            store.read()?.as_deref() == Some(initial.as_str()),
            "fixture mismatch"
        ),
        "update" => {
            ensure!(
                store.read()?.as_deref() == Some(initial.as_str()),
                "fixture mismatch"
            );
            store.write(&updated)?;
        }
        "read-updated" => {
            ensure!(
                store.read()?.as_deref() == Some(updated.as_str()),
                "fixture mismatch"
            )
        }
        "native-denied" => ensure!(
            native.get_password().is_err(),
            "direct native reader was accepted"
        ),
        "legacy-create" => {
            ensure!(
                matches!(native.get_password(), Err(keyring::Error::NoEntry)),
                "legacy fixture already exists"
            );
            native.set_password(&initial)?;
        }
        "legacy-delete" => {
            ensure!(native.get_password()? == initial, "legacy fixture mismatch");
            native.delete_credential()?;
        }
        "delete" => {
            let value = store.read()?;
            ensure!(
                value.as_deref() == Some(initial.as_str())
                    || value.as_deref() == Some(updated.as_str()),
                "refused to delete changed fixture"
            );
            store.delete()?;
        }
        _ => bail!("unknown probe operation"),
    }
    println!("native keychain {operation}: passed (build {variant})");
    Ok(())
}
