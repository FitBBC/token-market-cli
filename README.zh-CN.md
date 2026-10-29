# Token Market Agent CLI

[English](README.md) | [简体中文](README.zh-CN.md)

通过终端配置 Token Market，或在桌面端登录一次后持续管理六种本机 Agent。终端按每次 setup 进行授权；桌面登录后进入平台 Default Workspace，点击接入时才签发独立的设备/工具 Key。详见[桌面登录](docs/desktop-login.md)与[桌面管理](docs/desktop-management.md)。

## 安装

### 终端用户

安装 Node.js 20 或更新版本后，可通过 npm 分发入口运行 CLI，无需安装 Rust 或桌面 App：

```bash
npx --yes tokenmarket setup
npx --yes tokenmarket setup claude-code
npx --yes tokenmarket doctor
```

也可以全局安装，再直接使用 `tokenmarket` 命令：

```bash
npm install -g tokenmarket
tokenmarket setup
```

上述命令需要先将当前版本发布到 npm；仓库中的启动器本身不代表已经完成发布。发布步骤见 [CLI npm 发布](docs/cli-npm-release.md)。

支持 macOS Apple Silicon / Intel、Windows x64，以及使用 glibc 2.35+ 的 Linux x64 / arm64（如 Ubuntu 22.04+）。npm 自动安装当前平台的预编译 CLI，请保留 optional dependencies。Alpine/musl 或其他架构可从源码构建。

先决条件：

- 运行 setup 前，请先安装支持的 AI Agent 工具。CLI 只会配置它能检测到的工具。
- 如果要从源码构建 CLI，请安装包含 Cargo 的稳定版 Rust 工具链。
- `tokenmarket` 可执行文件必须在 `PATH` 中。

从源码安装可执行文件：

```bash
cargo install --path rust/tokensmarket-cli
```

Cargo 通常会安装到 `~/.cargo/bin`；请确认该目录在 `PATH` 中。也可以只构建、不安装，然后通过完整路径调用：

```bash
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
rust/tokensmarket-cli/target/release/tokenmarket --version
```

### 桌面用户

从 GitHub Releases 下载引导式桌面应用：

- **macOS**：下载 `.dmg`，打开后把 **Token Market** 拖入 **Applications**。
- **Windows**：下载 `x64-setup.exe` 安装器并运行。

应用尚未使用 Apple Developer ID 签名和公证。如果 macOS 首次启动拦截，请执行：

```bash
xattr -cr "/Applications/Token Market.app"
```

然后正常打开应用。

## 桌面端快速开始

在浏览器登录后通过 **Open Application** 返回桌面端。查看余额或切换 Current Workspace，然后接入已安装工具。工具卡片保留所属账号与 Bound Workspace。接入不自动调用付费模型；**测试**是独立动作，需要先确认可能产生的费用。

## 终端快速开始

不带参数运行 setup，会交互式列出可安装工具：

```bash
tokenmarket setup
```

也可以直接指定工具：

```bash
tokenmarket setup claude-code
```

配置完成后，检查本地安装状态和网关连通性：

```bash
tokenmarket doctor
tokenmarket test claude-code
```

## 支持的工具

| 工具 | Tool ID | 配置位置 |
| --- | --- | --- |
| Claude Code | `claude-code` | `~/.claude/settings.json` |
| Codex CLI | `codex` | `~/.codex/config.toml` |
| WorkBuddy | `workbuddy` | `~/.workbuddy/models.json` |
| DeepSeek Harness | `deepseek-harness` | `~/.dsh/profiles/<active-profile>/cordis.patch.yml` |
| OpenClaw | `openclaw` | `~/.openclaw/openclaw.json` |
| OpenCode | `opencode` | `~/.config/opencode/opencode.json` |

CLI 会检测每个工具的实际配置路径。`CLAUDE_CONFIG_DIR`、`CODEX_HOME`、`DSH_HOME`、`DSH_ACTIVE_PROFILE`、`WORKBUDDY_CONFIG_PATH`、`OPENCLAW_CONFIG_PATH`、`OPENCODE_CONFIG` 可以覆盖默认位置。

## 命令

### `tokenmarket setup [tool]`

授权 Token Market，创建 CLI API Key，写入所选工具配置，并执行验证请求。

选项：

- `--model <preset>`：选择模型预设；默认值为 `recommended`。

示例：

```bash
tokenmarket setup codex
tokenmarket setup codex --model fast
```

### `tokenmarket doctor [tool]`

显示当前配置的网关地址、各工具的安装状态、配置文件路径以及模型 API 连通性。传入 tool ID 时只检查该工具。

```bash
tokenmarket doctor openclaw
```

### `tokenmarket test <tool>`

执行已配置的请求测试。不传测试参数时默认执行全部测试。

选项：

- `--models`：模型列表请求
- `--chat`：非流式聊天请求
- `--stream`：流式聊天请求
- `--tool-call`：工具调用请求

```bash
tokenmarket test codex --models --chat
```

### `tokenmarket keys`

列出本 CLI 管理的 API Key。命令会显示 Key 名称、Key ID、目标工具和创建时间，不会输出 API Key 密钥。

```bash
tokenmarket keys
```

## 模型

安装时使用模型预设，而不是让用户直接填写原始模型名。

| 预设 | 模型 |
| --- | --- |
| `recommended` | `glm-5.3` |
| `powerful` | `glm-5.3` |
| `cheap` | `glm-5.3` |
| `fast` | `deepseek-v4.1-flash` |

默认预设是 `recommended`。

## 安全、备份与撤销

- 终端 CLI 授权使用 PKCE，并通过 `http://127.0.0.1` 上的一次性本地回调完成。
- 终端 CLI 浏览器授权页允许已登录用户选择该 CLI API Key 所属的 workspace。
- 在 Unix-like 系统上，工具配置和凭据文件会以仅文件所有者可访问的权限写入。Windows 上隐私由用户配置目录和父目录 ACL 保证。
- 修改已有配置前，CLI 会先复制备份到 `~/.tokensmarket/backups/`。
- 本地 Key 登记文件位于 `~/.tokensmarket/keys.json`，只记录元数据，不会替代工具配置。
- 对同一工具重新安装会替换本地管理记录。如果不再希望某个 Key 继续生效，请在 Token Market 控制台撤销该 Key。

桌面登录使用 PKCE 与已注册的应用协议进行一次性回调，登录阶段不选择空间、不签发工具 Key。桌面凭证私密存储、字段级恢复、精确 Key 清理与测试边界见[桌面管理](docs/desktop-management.md)。

## 架构

- `rust/tokensmarket-core`：共享安装领域核心，包含工具适配器、PKCE 授权、配置发现与备份、Key 登记、模型解析、网关验证。
- `rust/tokensmarket-cli`：`tokenmarket` 可执行文件，是共享核心之上的薄命令行外壳。
- `npm`：npx / npm 启动器，选择当前平台的预编译 Rust CLI，透传命令参数、终端输入输出与退出码。
- `src-tauri`：原生桌面应用，同样是薄外壳，直接调用同一个 Rust core。
- `src/tauri-ui`：桌面应用的 HTML 界面。

## 开发

仓库根目录没有 Cargo workspace，因此 Cargo 命令需要显式指定目标 crate 的 manifest。

npm 启动器测试使用 Node.js 内置测试工具，无需安装 npm 依赖：

```bash
npm test
```

```bash
cargo fmt --manifest-path rust/tokensmarket-core/Cargo.toml --all --check
cargo test --manifest-path rust/tokensmarket-core/Cargo.toml --all-targets
cargo test --manifest-path rust/tokensmarket-cli/Cargo.toml --all-targets
cargo check --manifest-path src-tauri/Cargo.toml
```

构建发布产物：

```bash
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
cargo tauri build
```

`cargo tauri build` 只在本地需要产出桌面安装包时使用，不属于常规 Rust 开发检查。

## 平台协议

授权 API 默认地址是 `https://console-api.tokensapi.cn`（可用 `TOKEN_MARKET_CONSOLE_API_BASE_URL` 覆盖）；网关请求默认地址是 `https://api.tokensapi.cn`（可用 `TOKEN_MARKET_GATEWAY_API_BASE_URL` 覆盖）。

CLI 依赖以下平台端点：

- `POST /api/cli/sessions`
- `POST /api/cli/exchange`
- `/v1` 下的 OpenAI 兼容模型和聊天端点
