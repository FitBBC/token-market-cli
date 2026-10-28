# Token Market Agent CLI

[English](README.md) | [简体中文](README.zh-CN.md)

用一条命令为常用 AI Agent 工具配置 Token Market。同一套安装流程提供终端 CLI 和引导式桌面应用两种入口。

## 安装

### 终端用户

仓库当前没有发布独立的 CLI 安装包，请从源码构建可执行文件：

```bash
cargo build --release --manifest-path rust/tokensmarket-cli/Cargo.toml
```

构建产物位于 `rust/tokensmarket-cli/target/release/tokensmarket`。

### 桌面用户

从 GitHub Releases 下载引导式桌面应用：

- **macOS**：下载 `.dmg`，打开后把 **Token Market Setup** 拖入 **Applications**。
- **Windows**：下载 `x64-setup.exe` 安装器并运行。

应用尚未使用 Apple Developer ID 签名和公证。如果 macOS 首次启动拦截，请执行：

```bash
xattr -cr "/Applications/Token Market Setup.app"
```

然后正常打开应用。

## 快速开始

不带参数运行 setup，会交互式列出可安装工具：

```bash
tokensmarket setup
```

也可以直接指定工具：

```bash
tokensmarket setup claude-code
```

配置完成后，检查本地安装状态和网关连通性：

```bash
tokensmarket doctor
tokensmarket test claude-code
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

### `tokensmarket setup [tool]`

授权 Token Market，创建 CLI API Key，写入所选工具配置，并执行验证请求。

选项：

- `--model <preset>`：选择模型预设；默认值为 `recommended`。

示例：

```bash
tokensmarket setup codex
tokensmarket setup codex --model fast
```

### `tokensmarket doctor [tool]`

显示当前配置的网关地址、各工具的安装状态、配置文件路径以及模型 API 连通性。传入 tool ID 时只检查该工具。

```bash
tokensmarket doctor openclaw
```

### `tokensmarket test <tool>`

执行已配置的请求测试。不传测试参数时默认执行全部测试。

选项：

- `--models`：模型列表请求
- `--chat`：非流式聊天请求
- `--stream`：流式聊天请求
- `--tool-call`：工具调用请求

```bash
tokensmarket test codex --models --chat
```

### `tokensmarket keys`

列出本 CLI 管理的 API Key。命令会显示 Key 名称、Key ID、目标工具和创建时间，不会输出 API Key 密钥。

```bash
tokensmarket keys
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

- 授权使用 PKCE，并通过 `http://127.0.0.1` 上的一次性本地回调完成。
- 浏览器授权页允许已登录用户选择该 CLI API Key 所属的 workspace。
- 在 Unix-like 系统上，工具配置和凭据文件会以仅文件所有者可访问的权限写入。Windows 上隐私由用户配置目录和父目录 ACL 保证。
- 修改已有配置前，CLI 会先复制备份到 `~/.tokensmarket/backups/`。
- 本地 Key 登记文件位于 `~/.tokensmarket/keys.json`，只记录元数据，不会替代工具配置。
- 对同一工具重新安装会替换本地管理记录。如果不再希望某个 Key 继续生效，请在 Token Market 控制台撤销该 Key。

## 架构

- `rust/tokensmarket-core`：共享安装领域核心，包含工具适配器、PKCE 授权、配置发现与备份、Key 登记、模型解析、网关验证。
- `rust/tokensmarket-cli`：`tokensmarket` 可执行文件，是共享核心之上的薄命令行外壳。
- `src-tauri`：原生桌面应用，同样是薄外壳，直接调用同一个 Rust core。
- `src/tauri-ui`：桌面应用的 HTML 界面。

## 开发

仓库根目录没有 Cargo workspace，因此 Cargo 命令需要显式指定目标 crate 的 manifest。

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

## 当前限制

- 首个发布版本面向国内环境。
- 支持 OpenClaw。
- 不支持 qclaw，CLI 不会检测、配置或文档化 qclaw。
