# CLI npm 发布

`tokenmarket` 是一个携带五个平台预编译二进制的 npm 包。App 与 CLI 共用 `rust/tokenmarket-core`、管理登录、当前空间及工具连接。CLI 使用共享客户端的远程浏览器确认登录和工具 Key 生命周期协议，详见 [ADR 0004](adr/0004-shared-client-key-lifecycle.md)。

## 包结构

- `npm/bin/tokenmarket.cjs`：Node.js 20+ 启动器，提供 `tokenmarket` 命令。
- `npm/native/<os-arch>/bin/`：macOS arm64 / x64、Windows x64、Linux glibc x64 / arm64 的 Rust CLI。
- `npm/native/<os-arch>/manifest.json`：该二进制的版本、Rust target 和 SHA-256。

只发布 `tokenmarket` 一个 npm 包，1.0.0 压缩后约 17.5 MB。启动器在本地选择当前系统的二进制，不运行安装脚本，不在运行时下载二进制，不调用桌面 App，也不依赖 optional dependencies。生成的 `npm/native` 不提交到 Git。

## 本地验证

以下示例适用于 Apple Silicon macOS，只打包当前平台以便本地验收：

```bash
npm test
npm run build:cli
npm run package:platform -- darwin-arm64 rust/tokenmarket-cli/target/release/tokenmarket
npm pack --pack-destination . --ignore-scripts
npm run test:package -- .
```

其他平台替换 `darwin-arm64`；Windows 可执行文件名是 `tokenmarket.exe`。正式发布前必须汇总全部五个平台；发布脚本会拒绝缺少任何平台的 tarball。

最后一步使用临时本地 npm registry、独立缓存和真实 tarball，通过 npm exec（npx 的执行入口）安装，检查 `--version`、`--help` 与错误命令的退出码。安装时关闭安装脚本和 optional dependencies，验证它们不是启动条件。测试不授权、不读取 Agent 凭据、不修改工具配置，也不发起模型请求。

## CI 和正式发布

`.github/workflows/cli-npm.yml` 在相关 PR、main 提交和 `cli-v*` tag 上运行：

1. 在五个平台构建并测试 CLI，完成各自的 npx 安装验证，上传原生二进制与 manifest。
2. 汇总二进制，核对版本、Rust target 与 SHA-256，恢复 artifact 下载后丢失的执行权限，生成一个通用 npm tarball。
3. 在五个平台安装并运行这个相同的最终 tarball。
4. 全部通过后，才允许正式发布。

普通提交和默认的手动运行只构建、验证。`cli-v*` tag 或手动勾选 `publish` 才会正式发布。桌面端原有的 `.github/workflows/release.yml` 使用 `v*` tag，双方独立运行。

首次发布需要有权创建 `tokenmarket` 包的 npm 账号，以及仓库 Actions secret `NPM_TOKEN`。Token 需要允许发布，并满足 npm 的 2FA 要求。通过校验后，手动运行 **CLI npm packages** 并勾选 `publish`。

CLI 和 App 的版本现已统一重置为 1.0.0。`package.json`、`rust/tokenmarket-cli/Cargo.toml`、`rust/tokenmarket-core/Cargo.toml`、`src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json` 的版本必须一致。正式发布时，在同一提交上创建 CLI tag `cli-v1.0.0` 和桌面 tag `v1.0.0`，分别触发 npm 五平台验证／发布和桌面安装包构建。

发布脚本先检查 tarball 包名、版本与全部平台文件，再执行发布预演。失败后可以使用相同的 `npm-release` artifact 重试：已发布包的 integrity 完全一致才会跳过；如果同版本内容不同，必须使用新版本。

发布完成后，在空目录用公共 registry 验证：

```bash
npx --yes --registry=https://registry.npmjs.org tokenmarket@1.0.0 --version
npx --yes --registry=https://registry.npmjs.org tokenmarket@1.0.0 --help
```

完整浏览器授权和实际工具配置仍是独立验收步骤，需要用户真实登录。仅通过打包测试不能证明线上授权、Key 权限或模型请求可用。

## npm 注册表限制

首次尝试独立平台包时，npm 对 `tokenmarket-win32-x64` 返回 `Package name triggered spam detection`。最终分发方案只使用一个通用包；不再创建或依赖独立平台 npm 包。

npm Token 与后续的免 Token 发布配置参见 [npm access tokens](https://docs.npmjs.com/creating-and-viewing-access-tokens/) 和 [Trusted Publishing](https://docs.npmjs.com/trusted-publishers/)。

## 旧版本清理（维护者要求的 1.0.0 重置）

2026-11-03 先发布并核对 `tokenmarket@1.0.0` 和 `latest`，再逐个删除 `0.0.0-stage`、`0.1.0`、`1.1.0`、`1.1.1`。官方 registry 最终确认版本清单仅为 `["1.0.0"]`，`latest` 为 `1.0.0`；保留版本的 integrity 与实际发布 tarball 一致。

仓库的发布 Token 可以发布，但 npm 拒绝其执行 unpublish，要求交互式二次验证。此次清理由维护者完成官方浏览器验证后执行，已移除一次性清理工作流。未来清理仍需先备份、核对保留版本及 latest，再按 npm 当时要求完成认证；不能把“弃用”当作已删除。旧内容归档、删除回执与最终 registry 校验位于本地 `dist/release-reset-20261103/`。
