//! The Apple-signed security tool is the stable caller for locally signed builds.
//! Secrets are transported over pipes, never shell commands or process arguments.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

const SECURITY: &str = "/usr/bin/security";
const PREFIX: &str = "tokenmarket-keychain-base64-v1:";

fn identifiers(service: &str, account: &str) -> Result<()> {
    // The interactive security command has its own parser. Only these fixed
    // service names and hashed accounts are accepted; no quoting is required.
    if [service, account].iter().any(|value| {
        value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    }) {
        bail!("系统凭据标识无效")
    }
    Ok(())
}

fn user_keychain() -> Result<PathBuf> {
    let output = Command::new(SECURITY)
        .args(["default-keychain", "-d", "user"])
        .output()
        .context("无法访问系统钥匙串")?;
    if !output.status.success() {
        bail!("无法访问系统钥匙串，请检查登录钥匙串")
    }
    let path = std::str::from_utf8(&output.stdout)
        .context("系统钥匙串路径无效")?
        .trim()
        .trim_matches('"');
    if !path.starts_with('/') {
        bail!("系统钥匙串路径无效")
    }
    Ok(PathBuf::from(path))
}

fn decode(bytes: &[u8]) -> Result<String> {
    let raw = std::str::from_utf8(bytes).context("系统凭据编码无效")?;
    // security prints a line ending after the value. New values are base64, so
    // arbitrary UTF-8 (including trailing whitespace) round-trips losslessly.
    let raw = raw.trim_end_matches(['\r', '\n']);
    match raw.strip_prefix(PREFIX) {
        Some(encoded) => String::from_utf8(STANDARD.decode(encoded).context("系统凭据编码无效")?)
            .context("系统凭据编码无效"),
        None => Ok(raw.to_owned()), // Existing raw native records remain readable after authorization.
    }
}

pub(super) fn read(service: &str, account: &str) -> Result<Option<String>> {
    identifiers(service, account)?;
    let output = Command::new(SECURITY)
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .arg(user_keychain()?)
        .output()
        .context("无法读取系统钥匙串")?;
    match output.status.code() {
        Some(0) => Ok(Some(decode(&output.stdout)?)),
        Some(44) => Ok(None), // errSecItemNotFound; never mistake access failure for missing data.
        _ => bail!("无法读取系统私密存储，请解锁钥匙串或批准旧凭据的访问"),
    }
}

fn write_command(
    service: &str,
    account: &str,
    value: &str,
    keychain: &std::path::Path,
) -> Result<String> {
    identifiers(service, account)?;
    let encoded = STANDARD.encode(value.as_bytes());
    let path = keychain.to_str().context("系统钥匙串路径无效")?;
    if path.contains(['\r', '\n', '\0']) {
        bail!("系统钥匙串路径无效")
    }
    let path = path.replace('\\', "\\\\").replace('"', "\\\"");
    // Do not pass -T, even with the same trusted application: updating an
    // existing item's access invokes SecKeychainItemSetAccess and prompts on
    // every session renewal. New items trust their creator (security) by
    // default; updates must preserve the item's existing access permissions.
    let command = format!(
        "add-generic-password -U -s {service} -a {account} -w {PREFIX}{encoded} \"{path}\"\n"
    );
    // security's interactive input uses a bounded line buffer. Reject rather
    // than accepting a partial credential; no ordinary-file fallback is used.
    if command.len() > 4095 {
        bail!("登录凭据超过系统钥匙串工具的单次写入限制")
    }
    Ok(command)
}

pub(super) fn write(service: &str, account: &str, value: &str) -> Result<()> {
    let keychain = user_keychain()?;
    let command = write_command(service, account, value, &keychain)?;
    let mut child = Command::new(SECURITY)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("无法保存系统凭据")?;
    let sent = child
        .stdin
        .take()
        .context("无法保存系统凭据")?
        .write_all(command.as_bytes());
    if sent.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        bail!("无法保存系统凭据")
    }
    if !child.wait()?.success() || read(service, account)?.as_deref() != Some(value) {
        bail!("系统凭据未成功保存，请检查钥匙串权限")
    }
    Ok(())
}

pub(super) fn delete(service: &str, account: &str) -> Result<()> {
    identifiers(service, account)?;
    let output = Command::new(SECURITY)
        .args(["delete-generic-password", "-s", service, "-a", account])
        .arg(user_keychain()?)
        .output()
        .context("无法移除系统登录凭据")?;
    match output.status.code() {
        Some(0 | 44) => Ok(()),
        _ => bail!("无法移除系统登录凭据"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_session_updates_preserve_existing_access_permissions() {
        // Session renewal persists a changed expiry on every refresh. Requesting
        // trusted applications here makes security reset the ACL each time,
        // producing a new "change access permissions" password prompt.
        for value in ["initial-session", "renewed-session", "renewed-again"] {
            let command = write_command(
                "cn.tokenmarket.setup.desktop-session",
                "platform-123",
                value,
                std::path::Path::new("/tmp/test.keychain-db"),
            )
            .unwrap();
            assert!(command.starts_with("add-generic-password -U "));
            assert!(
                !command.contains(" -T ") && !command.contains(" -A "),
                "session persistence must not request access permission changes"
            );
        }
    }

    #[test]
    fn credential_content_round_trips_without_entering_the_command_parser() {
        let secret = "含空格 ' \" ` $() \\ \n尾部\r\n";
        let command = write_command(
            "cn.tokenmarket.fixture",
            "platform-123",
            secret,
            std::path::Path::new("/tmp/a b.keychain-db"),
        )
        .unwrap();
        assert!(!command.contains(secret));
        let encoded = format!("{PREFIX}{}\n", STANDARD.encode(secret.as_bytes()));
        assert_eq!(decode(encoded.as_bytes()).unwrap(), secret);
        assert_eq!(decode(b"legacy-session\n").unwrap(), "legacy-session");
        assert!(decode(format!("{PREFIX}invalid!").as_bytes()).is_err());
    }

    #[test]
    fn invalid_identifiers_and_oversize_values_are_rejected_before_spawning() {
        for invalid in ["", "x y", "x\nquit", "x'", "x;", "x\\"] {
            assert!(write_command(
                invalid,
                "platform-123",
                "secret",
                std::path::Path::new("/tmp/test.keychain-db")
            )
            .is_err());
            assert!(write_command(
                "cn.tokenmarket.fixture",
                invalid,
                "secret",
                std::path::Path::new("/tmp/test.keychain-db")
            )
            .is_err());
        }
        assert!(write_command(
            "cn.tokenmarket.fixture",
            "platform-123",
            &"a".repeat(4096),
            std::path::Path::new("/tmp/test.keychain-db")
        )
        .is_err());
    }
}
