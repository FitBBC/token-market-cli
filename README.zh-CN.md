# Token Market

[English](README.md) | [简体中文](README.zh-CN.md)

将本机 AI Agent 工具接入 Token Market。可以使用桌面应用管理连接，也可以通过终端完成配置。

开始前，请先安装要接入的 Agent 工具，并准备好 Token Market 账号。

## 桌面端使用

### 安装

从 [GitHub Releases](https://github.com/FitBBC/token-market-cli/releases) 下载桌面安装包：

- **macOS**：下载 `.dmg`，打开后将 **Token Market** 拖入 **Applications**。
- **Windows**：下载 `x64-setup.exe` 并运行安装。

### 登录并接入工具

1. 打开 Token Market，点击 **使用浏览器登录**。
2. 在浏览器完成登录，点击 **Open Application** 返回桌面应用。
3. 确认当前空间。该空间用于本次新连接的计费。
4. 找到已安装的工具，点击 **接入**。刚安装的工具可先点击 **重新检测**。
5. 接入完成后，重新打开 Agent 工具，并按提示选择 Token Market 模型。

接入只写入配置，不会自动发送模型请求。需要确认连接是否可用时，在连接详情中点击 **测试**，选择模型（如有多个），确认费用后执行。

### 管理已有连接

- **重新接入**：使用当前账号和当前空间更新连接。需要更换连接所属空间时，先切换当前空间，再重新接入。
- **取消接入**：恢复本次连接管理的配置，并尝试停用对应 Key。遇到配置冲突时，按提示选择保留当前内容或恢复原值。
- **查看计费空间**：以工具连接详情中的计费空间为准。切换当前空间或退出桌面账号不会更改已有连接。

如果提示原 Key 未停用，请到 Token Market 控制台的 Key 管理中检查并手动停用。

## 终端使用

### 安装与运行

需要 **Node.js 20 或更新版本**。无需安装桌面应用，可直接运行：

```bash
npx --yes tokenmarket setup
```

也可以全局安装，之后使用 `tokenmarket` 命令：

```bash
npm install -g tokenmarket
tokenmarket setup
```

npm 方式适用于已发布到 npm 的版本；如果提示找不到包或版本，可先使用 [Releases](https://github.com/FitBBC/token-market-cli/releases) 中的桌面安装包。

CLI 支持 macOS Apple Silicon / Intel、Windows x64，以及使用 glibc 2.35+ 的 Linux x64 / arm64（如 Ubuntu 22.04+）。

以下示例使用全局安装后的命令；未全局安装时，将 `tokenmarket` 换成 `npx --yes tokenmarket` 即可。

### 接入工具

运行 `tokenmarket setup` 后，按提示选择工具，在浏览器登录并选择授权空间，然后返回终端等待配置完成。

也可以直接指定工具：

```bash
tokenmarket setup claude-code
tokenmarket setup codex
```

配置完成后，按终端提示重新打开 Agent 工具，并选择 Token Market 模型。

**CLI 的 setup 会自动发送验证请求，可能产生模型调用费用。**

### 选择模型

通过 `--model` 指定模型预设，默认使用 `recommended`：

```bash
tokenmarket setup codex --model fast
```

| 预设 | 模型 |
| --- | --- |
| `recommended`（默认） | `glm-5.3` |
| `powerful` | `glm-5.3` |
| `cheap` | `glm-5.3` |
| `fast` | `deepseek-v4.1-flash` |

### 检查配置与测试连接

检查所有工具，或只检查指定工具的配置与模型列表连通性：

```bash
tokenmarket doctor
tokenmarket doctor codex
```

发送请求测试指定工具：

```bash
tokenmarket test codex
tokenmarket test codex --models
tokenmarket test codex --chat --stream
```

| 测试参数 | 检查内容 |
| --- | --- |
| `--models` | 模型列表 |
| `--chat` | 非流式聊天 |
| `--stream` | 流式聊天 |
| `--tool-call` | 工具调用 |

不传测试参数时执行全部测试。聊天、流式和工具调用测试会发送真实模型请求，可能产生费用。

### 查看与停用 Key

```bash
tokenmarket keys
```

命令显示本 CLI 管理的 Key 名称、ID、目标工具和创建时间，不显示密钥内容。要停用 Key，请到 Token Market 控制台操作；重新运行 setup 不会自动撤销旧 Key。

## 支持的工具

桌面端可从工具列表接入，终端通过以下工具 ID 指定目标：

| 工具 | 终端工具 ID |
| --- | --- |
| Claude Code | `claude-code` |
| Codex CLI | `codex` |
| WorkBuddy | `workbuddy` |
| DeepSeek Harness | `deepseek-harness` |
| OpenClaw | `openclaw` |
| OpenCode | `opencode` |

## 常见问题

### macOS 首次打开被拦截

应用尚未使用 Apple Developer ID 签名和公证。确认安装包来自本仓库 Releases 后，可在终端执行：

```bash
xattr -cr "/Applications/Token Market.app"
```

然后重新打开应用。

### npm 提示缺少内置二进制

一个 npm 包内置五个平台的二进制。若安装不完整，请重新安装：

```bash
npm install -g tokenmarket
```

### 找不到已安装的工具

桌面端点击 **重新检测**。终端请确认目标工具可以从命令行启动，必要时重新打开终端再运行 setup。

### 接入后工具仍使用旧配置

完全退出并重新打开工具。OpenClaw 还需要重启 Gateway 或会话；OpenCode 的项目配置可能覆盖全局配置。

### 桌面端显示“配置已变化”或无法测试

检查是否在接入后修改了工具配置，按提示修复或重新接入，再执行测试。

### CLI 修改前的配置在哪里

CLI 在修改已有配置前会备份到 `~/.tokensmarket/backups/`，setup 的输出也会显示备份路径。
