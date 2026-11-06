# 桌面端构建、官网下载与在线更新发布

日常发布统一使用 `scripts/release-desktop.py`。用户告诉代理版本号和本次要包含的改动，
代理负责准备干净源码、构建、签名、上传 OSS、设置元数据和线上验证。
不用逐次登录 OSS 控制台，也不使用 GitHub CI 保存发布凭据。

当前 **1.0.0 首包已经上线**。下次只需告诉代理“发布 1.0.1，包含哪些改动”；
已有域名、OSS profile、长期签名身份无需重填。首包和后续版本共用下面的发布入口：
首次创建版本目录，后续新增版本目录、替换官网固定包，最后更新清单。

## 当前约定

- 下载域名：`https://dl.tokensapi.cn`；源站：阿里云 OSS。
- Mac：Apple Silicon `.dmg`，在线更新为 `.app.tar.gz`。
- Windows：x64 当前用户 NSIS `.exe`；可在 Mac 上交叉构建，不要求有 Windows 电脑。
- 官网入口 `/download` 自动识别 Windows/Mac；手机、iPad、Linux 显示手动选择。
- Mac 当前只提供 Apple Silicon 包，入口会说明 Intel Mac 暂不支持。
- 官网固定包名不含版本号；应用内更新使用不可变的版本包和签名。
- 保持已上线的目录，不再为缩短在线更新路径迁移目录。
- 不生成“官方安装包与恢复说明”网页；已有原生恢复协议属于另一个技术边界。

```text
OSS bucket root
├── download                                  # 官网按钮的固定链接
├── TokenMarket-mac-arm64.dmg                  # 官网当前 Mac 包，每次替换
├── TokenMarket-windows-x64.exe                # 官网当前 Windows 包，每次替换
├── desktop-updates/production/stable.json     # 应用内更新清单，最后发布
└── releases/1.0.0/                           # 历史版本不覆盖
    ├── TokenMarket-mac-arm64.dmg
    ├── TokenMarket-mac-arm64.app.tar.gz
    ├── TokenMarket-mac-arm64.app.tar.gz.sig
    ├── TokenMarket-windows-x64.exe
    └── TokenMarket-windows-x64.exe.sig
```

`desktop-updates/releases/...` 等更早发布的链接可继续保留兼容。
客户端读取的 `stable.json` 决定当前更新包地址；不要仅根据文件夹名字判断是否在使用。

官网首页只需添加或修改一个链接（官网代码需要另行部署）：

```html
<a href="https://dl.tokensapi.cn/download">下载客户端</a>
```

## 一次性配置

把 [JSON 模板](client-update-local.example.json) 复制到 Git 外，例如：

```text
~/tokenmarket-release-private/
  config-mac.json
  keys/                 # 长期 updater 公私钥、可选密码文件
  oss/config            # ossutil 的具名 profile
  tools/                # Tauri、ossutil、原生校验器
  build-cache/          # Windows 编译/SDK 缓存，允许复用
  outputs/版本号/       # 最终包、构建记录和待发布目录
  records/              # 构建、上线验证、清理记录
```

| 配置 | 填什么 |
| --- | --- |
| `source_root`, `version` | 干净源码绝对路径、桌面稳定版本；命令行可指定本次值 |
| `public_base_url` | `https://dl.tokensapi.cn/desktop-updates`，无末尾 `/` |
| `oss.prefix`, `manifest_key` | `desktop-updates`、`production/stable.json` |
| `oss.region`, `endpoint`, `bucket` | 实际 OSS 区域、HTTPS 服务 endpoint、桶名称 |
| `oss.config_file`, `profile`, `ossutil` | Git 外原生 ossutil 配置、具名 profile、ossutil 可执行文件 |
| `public_key_file`, `private_key_file` | 同一套长期 updater 签名身份；禁止每次打包重新生成 |
| `private_key_password_file` | 私钥密码文件，无密码时留空 |
| `output_dir` | Git 外输出目录，不能用仓库内 `dist` 放私有发布材料 |
| `tools.signer`, `native.tauri_cli_js` | Tauri CLI 2.12.4 可执行文件和其 JS 入口 |
| `tools.verifier` | 下述官方校验器的本机可执行文件 |
| `native.macos_identity_dir` | 既有持久 Mac 签名身份目录，保留原证书和 keychain |
| `native.windows_*` | 仅原生 Windows 代码签名使用；当前 Mac 交叉构建可留空 |
| `candidate_key`, `recovery_key` | 保留非空兼容值，如 `test/release-1.0.1/stable.json`、`recovery/index.html`；当前主流程不上传这两个页面/清单 |

公钥/域名是公开构建信息；私钥、密码、AccessKey、证书身份、实际配置和产物都留在 Git 外。
源码只提交模板、脚本、下载页模板和文档。不要把密钥发到聊天。

ossutil profile 示例（只在本机填写）：

```ini
[profile tokenmarket-release]
mode=AK
region=cn-beijing
endpoint=https://oss-cn-beijing.aliyuncs.com
access-key-id=本机填写
access-key-secret=本机填写
```

使用有目标桶对象读写和对象 ACL 修改权限的 RAM 凭据。发布程序不删除云端对象。
同一桶由一台指定发布机/账号操作；OS 锁不能跨主机互斥。

## 工具准备

需要 Python 3.9+、Rust、Node、macOS 开发工具，以及 Windows 交叉构建所需的
`cargo-xwin`、Windows Rust target、LLVM（`llvm-rc` / `clang-cl`）、LLD、NSIS、`7zz`。
使用 Tauri CLI **2.12.4**（bundler **2.10.4**）、ossutil **2.4.1**，不要静默漂移版本。
Mac 的 Homebrew LLVM 目录会自动加入构建进程 PATH，不修改用户 shell 配置。

从当前源码构建校验器，输出仍放在 Git 外：

```sh
CARGO_TARGET_DIR="$PRIVATE_DIR/verifier-target" cargo build --locked   --manifest-path src-tauri/Cargo.toml --example verify-update-artifacts
```

把 `verifier-target/debug/examples/verify-update-artifacts` 填到 `tools.verifier`。
它使用官方 updater **2.14.1**，只解析清单/验证签名，不启动安装器。

```sh
python3 scripts/release-desktop.py doctor --config "$PRIVATE_CONFIG"
```

首次建立 Mac/updater 签名身份应单独进行并备份，已有身份不能轮换来修复打包错误。

## 每次发布：代理执行

1. 确定要包含的完整源码提交。保护其他聊天的未提交内容，用独立 Git worktree 构建。
2. 将 `src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 和相关 Cargo.lock 中桌面版本改成
   本次稳定版本并提交。CLI 版本独立，不必一起变更。两个平台必须同一提交和版本。
3. 执行 `doctor`，确认工具、长期签名身份与凭据存在。版本源码应干净且包含本脚本。
4. 执行一条发布命令：

```sh
python3 scripts/release-desktop.py release   --config "$PRIVATE_CONFIG" --source "$RELEASE_SOURCE" --version 1.0.1   --notes-file "$PRIVATE_RELEASE_NOTES"
```

`RELEASE_SOURCE` 是正在运行此脚本的干净 checkout。不要从旧源码脚本构建新 checkout。
上述变量分别指本机私有配置路径、发布源码 worktree 和 UTF-8 更新说明文件；本机配置现位于
`/Users/bingbing.chen/tokenmarket-release-private/config-mac.json`。示例 `1.0.1` 需换成本次版本。
程序依次完成 Mac 构建与持久身份签名、Windows NSIS 交叉构建、更新文件签名、
九个公共文件的整理、OSS 上传和匿名读回校验。

发布顺序固定为：**版本文件 → 官网固定包和下载入口 → stable.json**。
每一步都完成读回验证后才进入下一步。失败不推进清单；清单 PUT 后读回失败会明确提示
线上入口可能已前进，不能把报错理解为已回滚。

上传不包含本地 `public/` 这一层目录名，也不包含配置、记录或 keys。
所有 Content-Type、Content-Disposition、Cache-Control、公共读 ACL 都由脚本设置。

5. 检查结果记录和浏览器下载；报告真实完成的证据，不能把构建或发布视为设备升级验收。

## 分步执行、重试与修元数据

构建失败先修失败原因，不要重新生成密钥、换域名或修改已发布版本的包字节。
已经完成的最终包可以分步整理/发布，无需重复编译：

```sh
python3 scripts/release-desktop.py build --config "$PRIVATE_CONFIG"   --source "$RELEASE_SOURCE" --version 1.0.1

python3 scripts/release-desktop.py stage --config "$PRIVATE_CONFIG"   --mac-dir "$MAC_OUTPUT" --windows-dir "$WINDOWS_OUTPUT"   --output "$NEW_STAGE_DIR" --notes-file "$PRIVATE_RELEASE_NOTES"

python3 scripts/release-desktop.py publish --config "$PRIVATE_CONFIG"   --plan "$NEW_STAGE_DIR/upload-map.json"

python3 scripts/release-desktop.py check --config "$PRIVATE_CONFIG"   --plan "$NEW_STAGE_DIR/upload-map.json"

python3 scripts/release-desktop.py metadata --config "$PRIVATE_CONFIG"   --plan "$NEW_STAGE_DIR/upload-map.json" --key TokenMarket-mac-arm64.dmg
```

`stage` 拒绝混用不同提交/版本或 fixture 与正式记录；`publish` 拒绝旧版本覆盖新版、
同版本路径下不同字节的覆盖。修改已发布包需发新版本，不复用版本目录。
`check` 不需要 OSS 写凭据、不修改线上数据。`metadata` 先核对线上哈希，只改选定文件的属性。

`build` 会复用同提交、同版本、同编译配置的已完成输出；只重试某个平台可添加
`--target mac` 或 `--target windows`。`stage` 不覆盖已存在目录；已经有 `upload-map.json`
时直接重试 `publish`，不要重跑完整 `release`。Windows SDK 缓存首次使用时可能下载较久；
本机原有 `windows-cross/cargo-target` 和 `windows-cross/xwin-cache` 会继续复用。

原生 Windows 已签名产物也可传给 `stage`；在 Mac 发布时用 `build/release --windows-dir`
复用同提交的原生 Windows 最终目录。底层 Windows 构建仍使用 `scripts/build-windows-clients.ps1`。
严格候选/设备验收流程见 [原生验收契约](client-update-promotion.md)，不与当前主入口混用。

## 元数据与验证边界

| 对象 | Content-Type | Cache-Control |
| --- | --- | --- |
| `download` | `text/html; charset=utf-8`，inline | `no-cache, no-store, max-age=0, must-revalidate` |
| 官网根目录 `.dmg` / `.exe` | `application/octet-stream`，固定下载文件名 | 同上 |
| `stable.json` | `application/json` | 同上 |
| 版本目录 `.dmg` / `.exe` | `application/octet-stream` | `public, max-age=31536000, immutable` |
| 版本目录 `.app.tar.gz` | `application/gzip` | 同上 |
| `.sig` | `text/plain` | 同上 |

Content-Encoding 留空，尤其不能给已签名压缩包设置 `gzip` 后再转换字节。
当前版本与清单相等时，两平台都应显示已是最新版本；较低版本应发现目标版本。
官网固定包替换不能代替更新 `stable.json`，清单也不能引用会被覆盖的根目录固定包。

以下证据分别记录：源提交/版本、包哈希、Mac 平台签名/公证、updater 文件签名、OSS 发布、
浏览器下载、真实设备安装/升级。当前 Mac 本地持久证书签名未 Apple 公证；Windows 交叉包
未配置 Authenticode，Windows 真机安装和跨版本升级尚未验收。自动脚本不会把这些状态改成通过。

## 清理约定

可清理已确认属于本次任务的失败包、错误域名包、重复的下载副本、旧临时脚本和冗余 worktree。
保留长期密钥/平台身份、实际配置、正式最终包、精简的构建/验证记录和有效 SDK/编译缓存。
线上不可变版本及兼容链接不作为日常清理对象；误上传的多余前缀只在确认无引用且内容属于本次产物后处理。
