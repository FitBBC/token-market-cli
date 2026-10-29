# CLI npm 发布

`tokenmarket` 是一个携带五个平台预编译二进制的 npm 包。App 与 CLI 继续共用 `rust/tokensmarket-core`；CLI 沿用 `/api/cli/sessions`、浏览器 `/cli/authorize` 和 `/api/cli/exchange` 的 PKCE 授权与 API Key 签发链路。

## 包结构

- `npm/bin/tokenmarket.cjs`：Node.js 20+ 启动器，提供 `tokenmarket` 命令。
- `npm/native/<os-arch>/bin/`：macOS arm64 / x64、Windows x64、Linux glibc x64 / arm64 的 Rust CLI。
- `npm/native/<os-arch>/manifest.json`：该二进制的版本、Rust target 和 SHA-256。

只发布 `tokenmarket` 一个 npm 包，压缩后约 13 MB。启动器在本地选择当前系统的二进制，不运行安装脚本，不在运行时下载二进制，不调用桌面 App，也不依赖 optional dependencies。生成的 `npm/native` 不提交到 Git。

## 本地验证

以下示例适用于 Apple Silicon macOS，只打包当前平台以便本地验收：

```bash
npm test
npm run build:cli
npm run package:platform -- darwin-arm64 rust/tokensmarket-cli/target/release/tokenmarket
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

`package.json` 与 `rust/tokensmarket-cli/Cargo.toml` 的版本必须一致。CLI tag 必须为 `cli-v<version>`，例如 `cli-v0.1.0`。CLI 和 App 的版本独立演进。

发布脚本先检查 tarball 包名、版本与全部平台文件，再执行发布预演。失败后可以使用相同的 `npm-release` artifact 重试：已发布包的 integrity 完全一致才会跳过；如果同版本内容不同，必须使用新版本。

发布完成后，在空目录用公共 registry 验证：

```bash
npx --yes --registry=https://registry.npmjs.org tokenmarket@0.1.0 --version
npx --yes --registry=https://registry.npmjs.org tokenmarket@0.1.0 --help
```

完整浏览器授权和实际工具配置仍是独立验收步骤，需要用户真实登录。仅通过打包测试不能证明线上授权、Key 权限或模型请求可用。

## npm 注册表限制

首次尝试独立平台包时，npm 对 `tokenmarket-win32-x64` 返回 `Package name triggered spam detection`。最终分发方案只使用一个通用包；不再创建或依赖独立平台 npm 包。

npm Token 与后续的免 Token 发布配置参见 [npm access tokens](https://docs.npmjs.com/creating-and-viewing-access-tokens/) 和 [Trusted Publishing](https://docs.npmjs.com/trusted-publishers/)。
