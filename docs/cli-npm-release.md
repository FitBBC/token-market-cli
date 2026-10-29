# CLI npm 发布

`tokenmarket` npm 包只负责启动原生 CLI。App 与 CLI 继续共用 `rust/tokensmarket-core`；CLI 沿用 `/api/cli/sessions`、浏览器 `/cli/authorize` 和 `/api/cli/exchange` 的 PKCE 授权与 API Key 签发链路。

## 包结构

- `tokenmarket`：Node.js 20+ 启动器，提供 `tokenmarket` 命令。
- `tokenmarket-darwin-arm64`、`tokenmarket-darwin-x64`：macOS 原生可执行文件。
- `tokenmarket-win32-x64`：Windows x64 可执行文件。
- `tokenmarket-linux-x64`、`tokenmarket-linux-arm64`：Linux glibc 可执行文件，在 Ubuntu 22.04 构建。

平台包是固定版本的 optional dependencies，并声明 `os` / `cpu`（Linux 还声明 `libc`）。npm 只安装匹配的平台包；启动器不运行安装脚本，不在运行时下载二进制，不调用桌面 App。

## 本地验证

在当前平台构建 CLI，再打包。以下示例适用于 Apple Silicon macOS：

```bash
npm test
npm run build:cli
npm run package:platform -- darwin-arm64 rust/tokensmarket-cli/target/release/tokenmarket
npm pack ./dist/npm/tokenmarket-darwin-arm64 --pack-destination dist --ignore-scripts
npm run test:package -- dist
```

其他平台将 `darwin-arm64` 换成对应的包后缀；Windows 可执行文件名是 `tokenmarket.exe`。

最后一步使用临时本地 npm registry、独立缓存和真实发布 tarball，通过 npm exec（npx 的执行入口）安装启动器及平台依赖，检查 `--version`、`--help` 与错误命令的退出码。测试不授权、不读取 Agent 凭据、不修改工具配置，也不发起模型请求。

## CI 和正式发布

`.github/workflows/cli-npm.yml` 在相关 PR、main 提交和 `cli-v*` tag 上构建五个平台。每个平台运行启动器测试、Rust CLI 测试及真实 tarball 安装验证。全部成功后，汇总六个 tarball 到 `npm-release` artifact。

普通提交和默认的手动运行只构建、验证。`cli-v*` tag 或手动勾选 `publish` 才会正式发布。桌面端原有的 `.github/workflows/release.yml` 使用 `v*` tag，双方独立运行。

首次发布前：

1. 使用有权发布这六个 npm 包名的账号。npm 主包名为 `tokenmarket`。
2. 在仓库 Actions secrets 配置 `NPM_TOKEN`，使用对这六个包有发布权限的 npm token，并按 npm 账号的 2FA 策略允许 CI 发布。首次创建包时，token 还需要创建新包的权限。
3. 手动运行 **CLI npm packages**，保持 `publish=false`，先检查所有平台通过。
4. 核对待发布版本，再使用版本 tag 或手动 `publish=true` 发布。

`package.json` 的版本、其中五个平台依赖的固定版本，以及 `rust/tokensmarket-cli/Cargo.toml` 的版本必须一致。使用 tag 时还要求 tag 为 `cli-v<version>`，例如 `cli-v0.1.0`。CLI 和 App 的版本可以独立演进。

发布脚本先检查全部 tarball，并对全部包执行发布预演，再依次发布五个平台包，最后发布主启动器，避免用户安装到缺少平台二进制的入口。失败后可以使用相同的 `npm-release` artifact 重试：已发布包的 integrity 完全一致才会跳过；如果同版本内容不同，脚本会停止，需要使用新版本。

发布完成后，在空目录用公共 registry 验证：

```bash
npx --yes --registry=https://registry.npmjs.org tokenmarket@0.1.0 --version
npx --yes --registry=https://registry.npmjs.org tokenmarket@0.1.0 --help
```

完整浏览器授权和实际工具配置仍是独立验收步骤，需要用户真实登录。仅通过打包测试不能证明线上授权、Key 权限或模型请求可用。

## 依赖被省略时

如果安装时使用 `--omit=optional`，启动器会提示缺少当前平台包。重新安装并启用 optional dependencies：

```bash
npm install -g --include=optional tokenmarket
```

对于已缓存失败安装的 npx，可使用新的缓存目录重新运行，例如：

```bash
npx --yes --include=optional --cache ./tokenmarket-npx-cache tokenmarket --version
```

平台依赖机制依据 [npm package.json 文档](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/)，构建平台标签参见 [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。
