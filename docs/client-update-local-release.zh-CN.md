# 桌面客户端：首次打包与后续本地 OSS 发布

采用本地手动触发的自动化：在 macOS / Windows 原生主机分别构建、平台签名、
准备更新包；把两份最终产物汇总到一台指定发布机，上传候选，完成真实安装验收，
最后显式发布 stable。源站是阿里云 OSS。无需 GitHub 保存私有配置、签名密钥或
OSS 凭据，也无需 R2、GitHub 下载回退或额外更新后端。

目前仅支持 macOS Apple Silicon 和 Windows x64 currentUser NSIS 自动更新。
Windows MSI 仍可用于手动安装/恢复，不是自动更新目标，也不是每次构建的必需产物。
两平台最终清单和真实设备验收都不能省略。当前没有 Windows 验收设备，#38 和父任务
#30 仍未完成；本指南和自动化测试不代表已签名、公证、部署或通过设备升级。

## 一次性准备

先确定公开域名、稳定清单地址和恢复页地址，再制作任何正式包。需要以下资源：

| 资源 / 字段 | 内容和用途 |
| --- | --- |
| `source_root` | 当前运行脚本所属的干净 Git checkout 的绝对路径，两主机使用同一完整提交 |
| `version` | 本次桌面稳定版本，例如 `1.1.0`；必须同时匹配 `src-tauri/tauri.conf.json` 和 `src-tauri/Cargo.toml` 的 package.version |
| `public_base_url` | 自有 HTTPS 域名下、映射到 OSS prefix 的公开根地址，无账号、查询参数和 fragment |
| `manifest_key` | 固定 stable 路径，例如 `production/stable.json`，不能在 test/ 或 releases/ 下 |
| `recovery_key` | 固定 HTML 恢复页，例如 `recovery/index.html`，不能带产物摘要或位于 test/、releases/ 下 |
| `candidate_key` | 单独的候选清单，例如 `test/release-1.1.0/stable.json` |
| `public_key_file` | 既有 Tauri updater 公钥文件，内容会编译进两平台最终客户端 |
| `private_key_file` | 与该公钥对应的既有 updater 私钥文件；本机签名时读入子进程环境，不写入清单、日志或 Git |
| `private_key_password_file` | 可选的私钥密码文本文件；无密码时留空，读取时仅去除行尾换行 |
| `output_dir` | Git 外的构建、最终包及汇总清单目录；每个版本/目标不可覆盖 |
| `tools.signer` | 本机 Tauri CLI **2.12.4** 可执行文件绝对路径 |
| `tools.verifier` | 本机编译的官方 updater **2.14.1** 校验器绝对路径 |
| `oss.region`, `endpoint`, `bucket`, `prefix` | 阿里云 OSS 实际区域、HTTPS 服务地址、桶及对象前缀 |
| `oss.config_file`, `profile` | Git 外的原生 ossutil 凭据配置文件和明确选择的 profile |
| `oss.ossutil` | 本机 ossutil **2.4.1** 可执行文件绝对路径 |
| `native.macos_identity_dir` | 既有持久 macOS 签名身份目录；升级必须复用，不能自动初始化或轮换 |
| `native.tauri_cli_js` | Mac 上同一 Tauri CLI2.12.4 的 JS 入口绝对路径，供现有构建脚本调用 Node |
| `native.windows_certificate_sha1` | 已安装 Windows 代码签名证书的 SHA1 指纹，含其可用私钥/硬件签名提供者 |
| `native.windows_timestamp_url` | 所选提供者的 RFC3161 时间戳 URL，支持 HTTP/HTTPS，不接受 URL 内账号密码 |
| `native.windows_signtool` | Windows SDK 的 SignTool 绝对路径 |

还需原生编译环境（Rust、Node、Mac 开发工具或 Windows MSVC/WebView2/NSIS 构建依赖）、
既有 macOS 签名身份、Windows 证书/私钥访问，以及公开域名 TLS/CDN/OSS 路由。
配置缺失会报字段名，不会打印字段值或子进程密钥输出。Mac 构建不要求 Windows 的
真实资源，原生构建也不要求 OSS 凭据配置可用；汇总和发布仍要求两份最终产物。

如果尚未建立 updater 签名身份，由资源所有者在独立的私有环境完成一次性准备并备份。
已有客户端的 updater 公钥和 macOS 身份不得随意更换。本工具不会为正式发布生成密钥、
初始化签名身份、开通 OSS、导入证书或读取客户端登录/模型凭据。

Mac 默认复用 `build-macos-clients.sh` 的持久证书，并检查最终 App 和 DMG。
该现有本地签名流程的 `notarized` 是 **false**，不等同于 Developer ID 公证或
Gatekeeper 公共发行验收。保留真实状态；需要正式 Developer ID/公证时应另行完成
相应资源和流程，不能把布尔值改成 true 来代替。Windows 默认构建和签名 NSIS，
保留 currentUser 安装钩子与标记，并把相同的最终 NSIS 字节作为独立手动恢复文件。

## 私有目录和配置

例如放在用户目录下的私有发布根目录，整个目录不能位于任何 Git checkout 中：

```text
~/tokenmarket-release-private/
  config-mac.json / config-windows.json / config-publisher.json
  keys/                         # 既有 updater 公私钥、可选密码文件
  native/                       # 既有 Mac 签名身份；Windows 证书由系统/提供者持有
  oss/                          # ossutil 原生配置/profile
  outputs/                      # 每个版本、每个原生目标和汇总清单
  evidence/                     # 真机日志、截图、验收 JSON、发行说明
  records/                      # 可选操作员记录，目录约定，无配置字段
  verifier-target/              # 本机构建的官方校验器
```

复制仓库中的 [空 JSON 示例](client-update-local.example.json) 到上面目录后再填值。
不使用 shell `source`/`eval`。所有私有路径使用绝对路径；链接解析后进入任何 Git
checkout 的路径同样会被拒绝。macOS 身份目录下的证书、密码、keychain 和身份元数据
也逐一检查；可变的 `build.lock` 不允许是符号链接。两原生主机可使用不同本地路径，
但源码提交、版本、
生产 endpoint、固定 recovery 和公钥指纹必须完全一致。不要把填好的配置、profile、
密钥、输出或验收材料提交到 Git。

在每个运行发布命令的主机上构建校验器，输出也放 Git 外：

```sh
CARGO_TARGET_DIR="$PRIVATE_DIR/verifier-target" cargo build --locked \
  --manifest-path src-tauri/Cargo.toml --example verify-update-artifacts
```

所有 JSON 字段保留，但只填写本机使用的资源：`private_key_file` / 密码和 signer 仅构建需要；
`source_root` 仅构建需要，`output_dir` 仅构建/汇总需要；OSS 各项只在 candidate/publish
需要。操作员记录可自行放在 Git 外的 `records/`，无需配置字段。只读 validate 无需私钥、
签名器或 OSS profile。

将生成的 `debug/examples/verify-update-artifacts`（Windows 为 `.exe`）填入
`tools.verifier`。构建/包验证与原生设备验收分开记录。发布机使用专用 ossutil profile；
CLI 清除环境中遗留的 OSS/GitHub/updater 签名输入，仅注入本次配置选择的 profile。

## 首次包：先完成手动安装入口

1. 在独立干净 checkout 中更新并提交桌面 `src-tauri/Cargo.toml` 与
   `src-tauri/tauri.conf.json` 的版本，并同步需要变更的 Cargo.lock package 版本。
   两主机 checkout 同一个完整提交。CLI 的版本和更新渠道独立，无需因此强制升级 CLI。
2. 配置最终 HTTPS stable/recovery 地址和长期公钥。最终发布包始终编译生产地址，
   不能为了候选验收而改成测试地址，验收后也不能重编译或修改签名包。
3. 在原生主机执行下面的 `local-build`，汇总、上传候选后取得独立手动安装链接。
   现有不带 updater 的老客户端只能先手动安装一个支持 updater 的版本；安装时保留
   原数据目录、Client Session、Tool Connections、系统凭据和恢复备份。
4. 为升级验收单独制作**内部较低版本的已安装客户端**，例如内部 `1.0.1` → 最终
   `1.1.0`。内部客户端编译相同 updater 公钥，发现地址指向隔离候选 manifest；
   它不是本次最终发布清单中的包，也不能被当成最终包的 build.json。
5. 内部较低版本不能通过 `local-build` 冒充生产构建。应在单独的低版本干净源码中，
   明确设置 `TOKEN_MARKET_UPDATER_ENDPOINT` 为完整候选 HTTPS URL、
   `TOKEN_MARKET_UPDATER_RECOVERY_URL` 为固定恢复页和 `TOKEN_MARKET_UPDATER_PUBLIC_KEY`
   为同一公钥，用原生构建包装器制作内部安装包，Cargo/output/身份仍在 Git 外。
   Mac 包装器可设置 `TOKEN_MARKET_INDEPENDENT_CLI_VERSION=1`；Windows 包装器使用
   同一证书/SignTool 参数。记录该内部客户端的源码、编译配置、安装位置及签名身份。
6. 从实际安装的内部较低版本发现候选，经下载/安装两次明确同意升级到最终包，验证
   新进程启动及模板中的所有行为。另做老客户端手动 bootstrap 后再应用内升级的验收。
   两平台都要真实执行；没有 Windows 设备时保持 pending，不能正式提升 stable。

第一次 stable 公开发布前允许固定 stable URL 返回真正404。候选已具有独立手动安装和
恢复 HTML，不应提前写 stable 以便“测一下”。更改最终包或公钥/编译地址后必须重新
构建、签名并验收。

## 每次发布的固定命令

Mac 原生主机：

```sh
python3 scripts/desktop-update-release.py local-build \
  --config "$PRIVATE_MAC_CONFIG" --target darwin-aarch64
```

Windows x64 原生主机（可用 PowerShell 设置变量并调用 Python）：

```text
python scripts/desktop-update-release.py local-build --config PRIVATE_WINDOWS_CONFIG --target windows-x86_64-nsis
```

脚本先校验干净源码和版本，再实际调用原生 build/sign，准备最终更新包并签署版本。
输出在 `output_dir/VERSION/TARGET/`：`prepared.json`、`build.json`、最终 updater
文件/签名及手动安装包。缺失资源或构建失败不会生成可供发布的成功记录；已有输出不覆盖。
把两目标目录**原样**传到指定发布机 Git 外目录，保持文件名和记录不变。

```sh
python3 scripts/desktop-update-release.py local-inventory --config "$PRIVATE_CONFIG" \
  --prepared "$MAC_OUTPUT/prepared.json" --prepared "$WINDOWS_OUTPUT/prepared.json" \
  --notes-file "$PRIVATE_RELEASE_NOTES"
```

命令自动检查两份源码/版本/配置一致、签名和哈希有效，生成
`output_dir/VERSION/inventory/release.json` 和 `acceptance-draft.json`。
后者自动填充公共绑定和最终文件摘要，真机状态仍为 false/pending；不要手工编辑签名、
build.json 或 prepared.json。保留汇总目录为只读，验收记录复制到私有 evidence 目录编辑。

```sh
python3 scripts/desktop-update-release.py local-candidate \
  --config "$PRIVATE_CONFIG" --inventory "$INVENTORY"
```

仅写 `candidate_key`；生产 stable 和固定恢复别名不变。命令上传不可变的
`releases/VERSION/SHA256/filename` 包和版本恢复 HTML，逐一匿名 GET 校验哈希、大小、
类型、缓存和原始字节，最后才写候选清单。私有验收记录应引用真实日志/截图，可以使用
`{"path":"/绝对路径/真实日志.txt","sha256":"完整SHA256"}`，不要求公开日志 URL。
记录字段和检查内容见 [验收契约](client-update-promotion.md)。

完成两平台所有真实检查后，先只读验证，再明确发布：

```sh
python3 scripts/desktop-update-release.py local-validate \
  --config "$PRIVATE_CONFIG" --inventory "$INVENTORY" --acceptance "$ACCEPTANCE"
python3 scripts/desktop-update-release.py local-publish \
  --config "$PRIVATE_CONFIG" --inventory "$INVENTORY" --acceptance "$ACCEPTANCE"
```

`local-validate` 不读取上传凭据内容、不联网、不上传、不启动安装器；它检验同样的
证据和真实 updater 签名/签署版本。`local-publish` 才取得共用目的地 OS 锁，读取当前
stable，阻止旧版本覆盖新版、阻止相同版本不同清单，遇到损坏清单、鉴权或网络错误即停止。
先上传/读回全部不可变包和版本恢复页，再更新并读回固定恢复页，最后才 PUT/读回 stable。
固定恢复链接因此不会指向尚未验证的包。

## 发布机、路由与失败处理

同一 bucket/prefix 只指定一台机器的同一发布账号写入。候选和 stable 共用按目的地
派生的 OS 锁，跨工作目录和私有配置/state 路径仍互斥；锁文件在 Unix 的
`/tmp/tokenmarket-publisher-UID/` 或 Windows 的
`%LOCALAPPDATA%/Token Market/Release Publisher Locks/`。进程退出会自动解锁，
运行中不要删除锁文件。它不是跨机器/跨账号分布式锁；其他机器和未配置的 CI 必须禁写。

OSS/CDN 公开路由应保持最终包原字节，不能做压缩转换。不可变包使用
`public, max-age=31536000, immutable`；stable 和固定恢复页使用
`no-cache, no-store, max-age=0, must-revalidate`。类型、配置和故障细节见
[分发说明](desktop-update-distribution.md)。本地 fixture 即使填了凭据也只能写隔离
local-store，不能上传 OSS；旧 CI/GITHUB 环境和 schema1 记录不能用于真实本地发布。

网络失败先重试下载。应用无法启动时，从固定恢复页手动覆盖安装同版本或更新的受支持
官方包；Windows MSI 仅作手动恢复，注意安装所有权，优先使用现有 NSIS 类型。
保留用户数据和凭据，不能通过删数据/清凭据恢复。没有自动回滚承诺，旧版也不保证能读
新版写入的数据。stable PUT 后读回失败会明确提示发现入口可能已前进，应检查线上状态
后再重试；不能把报错理解为已回滚。源码、构建、平台签名/公证、updater 校验、上传和
实际设备升级应分别记录结果。
