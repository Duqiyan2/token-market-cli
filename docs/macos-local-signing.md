# macOS 构建与共享钥匙串签名

CLI 和桌面是同一个 Token Market Client 的两个入口。macOS 本地构建使用
**固定自签名证书、相同的 designated requirement（DR）**，让两端都满足
新建钥匙串记录的可信程序规则。无需 Apple Developer 账号，凭证仍存放在
系统钥匙串，工具 Key、连接和恢复记录继续使用原有加密存储。

固定自签名证书不提供 Developer ID、公证或 Gatekeeper 信任。
下载后首次打开应用的系统信任提示与钥匙串访问弹窗是两类不同问题。

## 使用正式名称构建

```bash
bash scripts/build-macos-clients.sh
```

此入口构建 `Token Market.app`、CLI 和 DMG，复用固定签名身份，保留正式
Bundle ID `cn.tokenmarket.setup`。后续升级必须继续使用同一个身份目录。
首次使用先显式运行 `python3 scripts/macos-local-signing.py init` 初始化身份。
正式构建入口不会自动生成身份，身份目录丢失或配置错误时停止，避免升级时换证。
不要交付 `cargo tauri build` 直接产生的临时签名 App 或 DMG；它们只是中间产物。
已有的同版正式 App 和 CLI 也可直接重新签名，不需要改变用户数据：

```bash
python3 scripts/macos-local-signing.py --profile production package \
  --app 'src-tauri/target/release/bundle/macos/Token Market.app' \
  --cli rust/tokenmarket-cli/target/release/tokenmarket \
  --output dist/signed-production-clients
```

`production` 和 `local` 分别只允许自己的 App/CLI 标识，同一证书下两组
程序也不互相授权。正式组为 `cn.tokenmarket.setup` 和
`cn.tokenmarket.setup.cli`。两组继续使用原来的凭据命名空间和存储格式。

## GitHub 发布所需的固定身份

正式桌面 Release 和可发布的 macOS npm CLI 必须配置相同的仓库设置：

- Secret `MACOS_SIGNING_CERTIFICATE`：包含代码签名证书和私钥的 PKCS#12
  文件，编码成单行 base64。
- Secret `MACOS_SIGNING_PASSWORD`：该 PKCS#12 文件的导出口令。
- Variable `MACOS_SIGNING_CERTIFICATE_SHA1`：预期签名证书的 40 位 SHA-1
  指纹（公开值），用于阻止导入错误身份或意外换证。

两条发布流程都使用 `--profile production --ci`。每次在隔离临时钥匙串
导入固定证书，验证指纹、有效期和无界面互读测试，给最终二进制签名后才
创建 DMG、npm 清单和校验和。完成或失败后删除临时签名钥匙串；不修改
默认证书信任和默认钥匙串。缺少配置、证书不匹配或测试失败时发布停止，
不生成新证书、不降级成临时签名。普通 PR 的 npm 验证包不用于发布，
因此无需访问发布私钥。

正式分发可提供长期保留的 Developer ID Application 身份；本地固定自签名
身份只能解决钥匙串身份连续性，不能消除 Gatekeeper 的首次打开限制。
当前脚本不做公证。证书指纹被固定在 DR 内，换证前必须设计凭据迁移，
不能在升级中悄悄重新生成证书。首次配置 CI 时应复用本地交付包所用身份，
避免本地包和 CI 包再次形成不同身份。

不含私钥的发布摘要 `signing-manifest.json` 保存指纹、共享规则和文件校验和。
不要把 PKCS#12、口令、签名钥匙串或私钥提交到仓库或交付给客户端用户。

## 构建 macOS ARM 客户端

```bash
bash scripts/build-local-clients.sh
```

需要 Rust、Python 3、Xcode Command Line Tools 及 Tauri CLI。如果没有安装
`cargo tauri`，可以通过 `TAURI_CLI_JS` 指向已安装的 `@tauri-apps/cli/tauri.js`。
可传第一个参数指定空的输出目录。此脚本先构建 CLI 和 App，再做
钥匙串互读测试、签名校验，最后生成 CLI、App、DMG 和签名摘要。
单独的 `build-local-desktop.sh` 是底层构建步骤；要交付共享签名包，使用
这里的成对构建入口。

默认身份保存在构建机器的
`~/Library/Application Support/Token Market/Local Signing/`。
首次构建创建，以后复用，不会因为日期、版本或输出目录不同而轮换。
目录权限为 0700，私有文件权限为 0600。使用独立的构建签名钥匙串；
不导入用户的登录钥匙串，不更改默认证书信任，也不更改默认钥匙串。
创建时会移除系统自动加入搜索列表的构建钥匙串，签名结束后将它锁定。

`TOKEN_MARKET_LOCAL_SIGNING_DIR` 可以指定仓库外的固定身份目录。
该目录包含构建私钥和解锁材料，必须单独安全备份，不得放进 Git、npm、
DMG 或其他交付包。丢失后重新生成证书会改变签名身份。

## 签名规则与边界

本地组的共享 DR 同时检查证书指纹，并且只接受以下两个签名标识：

- `cn.tokenmarket.setup.test`：本地桌面 App。
- `cn.tokenmarket.setup.cli.test`：本地 CLI 二进制。

这不是“允许所有应用访问”。临时签名程序、其他证书签名程序，以及同一
证书签名但标识不同的程序不满足规则。两端声明相同的 DR，且使用同一张
长期保留的证书；仅更改文件名、目录或重新编译不会改变这套身份。
登录凭据按配置地址独立保存，签名不改变连接行为。

打包只接受文档列出的本地 App 标识，保留原签名中的 entitlements、flags、runtime，
不添加共享访问组权限，不需要 provisioning profile。生成的签名摘要记录
公开证书指纹、共享规则和 CLI/DMG SHA-256，不包含私钥或任何登录凭证。

## 已构建产物重新签名

如果两个产物已是同一次源码构建，可直接复制、重新签名和制作 DMG。
原始产物保持不变，输出目录必须为空：

```bash
python3 scripts/macos-local-signing.py init
python3 scripts/macos-local-signing.py smoke
python3 scripts/macos-local-signing.py package \
  --app 'src-tauri/target/release/bundle/macos/Token Market Local.app' \
  --cli rust/tokenmarket-cli/target/release/tokenmarket \
  --output dist/local-signed-clients
```

直接调用 Python 时，非默认身份目录使用全局参数
`--identity-dir /absolute/path`，放在子命令之前。

## 原生验证与既有数据

`smoke` 编译两个不同的独立程序，签成 CLI 和桌面的同一共享 DR，使用
独立临时钥匙串和虚拟内容。每个进程在访问前关闭钥匙串交互；需要授权
弹窗会变成失败。覆盖双向读取和更新、进程重启、可执行文件移动、重新
编译后的签名更新，以及临时签名/其他标识程序被拒绝。它不读取真实登录
或工具凭证，不操作 Agent 配置，不发起网络登录或模型请求。

该测试验证系统签名与 ACL 行为；真实客户端仍需用同一对新包检查：
桌面登录后 CLI 复用、CLI 登录后桌面复用，以及更新后再次访问。

**旧临时签名包创建的钥匙串记录保留原 ACL，新签名不能绕过其授权。**
访问这些记录时可能仍出现一次授权；CLI 和桌面可能各需授权一次。不会静默
删除旧会话、加密主密钥、工具 Key 或恢复记录来掩盖弹窗，也不会重发 Key。
全新记录由这套共享签名身份创建后，两端正常互读不需要该授权弹窗。

## 依据

- [Apple：Sharing a designated requirement](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html)：允许协同程序使用相同 DR 共享钥匙串访问。
- [Apple：Understanding the Code Signature](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/AboutCS/AboutCS.html)：钥匙串支持自签名身份，但自签名不提供 Gatekeeper 分发信任。
- [Apple：Access Control Lists](https://developer.apple.com/documentation/security/access-control-lists)：可信应用满足 ACL 时直接访问，否则需要用户确认。
