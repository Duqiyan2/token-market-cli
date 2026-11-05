#!/usr/bin/env python3
"""Reset Token Market local authorization; --all-connections also removes recovery data.

Does not revoke server API keys or edit Agent configuration files. Close the App
and CLI first. Full reset intentionally creates no backup and retains device-id.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys

SESSION = "cn.tokenmarket.setup.desktop-session"
TOOL_KEYS = "cn.tokenmarket.setup.tool-credentials"
SECURITY = "/usr/bin/security"


def stopped():
    processes = subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True)
    for row in processes.splitlines():
        fields = row.strip().split(None, 1)
        if len(fields) != 2:
            continue
        executable = fields[1]
        if executable.endswith(("/Token Market.app/Contents/MacOS/app", "/Token Market Local.app/Contents/MacOS/app")) or Path(executable).name.startswith("tokenmarket"):
            raise RuntimeError("请先完全退出 Token Market App 和 CLI；只关闭窗口不会退出常驻进程")


def user_keychain():
    result = subprocess.run([SECURITY, "default-keychain", "-d", "user"], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError("无法确定用户默认钥匙串，未扩大清理范围")
    path = result.stdout.strip().strip('"')
    if not path.startswith("/"):
        raise RuntimeError("用户默认钥匙串路径无效")
    return path


def remove_item(keychain, service, account=None):
    args = [SECURITY, "delete-generic-password", "-s", service]
    if account is not None:
        args += ["-a", account]
    result = subprocess.run([*args, keychain], capture_output=True)
    if result.returncode == 44:
        return False
    if result.returncode:
        raise RuntimeError(f"无法清理 {service}，未操作其他应用的凭据")
    return True


def reset_local(home, keychain, platform_url, all_connections=False):
    root = home / ".tokenmarket"
    for path in (root, root / "client-login", root / "desktop-connections", root / "backups"):
        if path.is_symlink():
            raise RuntimeError("Token Market 状态目录是符号链接，已停止清理")
    if not all_connections:
        account = "platform-" + hashlib.sha256(platform_url.rstrip("/").encode()).hexdigest()
        pending = root / "client-login" / account.removeprefix("platform-") / "pending-login.json"
        if pending.parent.is_symlink():
            raise RuntimeError("Token Market 登录目录是符号链接，已停止清理")
        remove_item(keychain, SESSION, account)
        pending.unlink(missing_ok=True)
        return
    for service in (SESSION, TOOL_KEYS):
        for _ in range(100):
            if not remove_item(keychain, service):
                break
        else:
            raise RuntimeError("匹配凭据数量异常，已停止清理")
    for path in (root / "client-login", root / "backups"):
        if path.exists():
            shutil.rmtree(path)
    connections = root / "desktop-connections"
    if connections.exists():
        for item in connections.iterdir():
            if item.name == "device-id":
                continue
            if item.is_dir() and not item.is_symlink():
                shutil.rmtree(item)
            else:
                item.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform-url", default="https://console-api.tokensapi.cn")
    parser.add_argument("--all-connections", action="store_true", help="同时清除所有环境登录、工具连接、缓存 Key、恢复备份和加密主密钥；不创建备份")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("此命令仅适用于 macOS")
    stopped()
    reset_local(Path.home(), user_keychain(), args.platform_url, args.all_connections)
    print("本地登录已清理，请打开修复版 App 重新授权。")
    if args.all_connections:
        print("工具连接、缓存 Key、恢复备份和加密主密钥已清除；设备标识、工具自身配置及安装目录保留。")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
