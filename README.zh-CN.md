# Token Market

[English](README.md) | [简体中文](README.zh-CN.md)

通过桌面应用或终端，将本机 AI Agent 工具接入 Token Market。两端共用登录、当前空间和工具连接，可以随时切换使用。

**当前版本 1.1.0** · [下载桌面端](https://github.com/FitBBC/token-market-cli/releases/tag/v1.1.0) · [npm 包](https://www.npmjs.com/package/tokenmarket)

## 安装

开始前，请准备好 Token Market 账号，并安装要接入的 AI Agent 工具。

### 桌面端

从 [Releases](https://github.com/FitBBC/token-market-cli/releases/tag/v1.1.0) 下载安装包：

| 系统 | 安装方式 |
| --- | --- |
| macOS，Apple Silicon | 打开 `.dmg`，将 **Token Market** 拖入 **Applications**。 |
| Windows x64 | 运行 `x64-setup.exe` 安装程序。 |

### CLI

需要 **Node.js 20+**。支持 macOS arm64/x64、Windows x64，以及 glibc 2.35+ 的 Linux x64/arm64。

```bash
npm install -g tokenmarket@1.1.0
tokenmarket --version
```

也可以直接运行，无需全局安装：

```bash
npx --yes tokenmarket@1.1.0 login
```

npm 包内置全部五个平台的原生二进制，启动器会自动选择当前系统对应的版本。

## 接入第一个工具

### 使用桌面端

1. 打开 Token Market，点击 **使用浏览器登录**。
2. 在浏览器完成登录，点击 **Open Application** 返回应用。
3. 确认当前空间，新连接使用这个空间计费。
4. 找到目标工具，点击 **接入**。刚安装的工具可以点击 **重新检测**。
5. 重新打开工具，按提示选择 Token Market 模型。

### 使用终端

```bash
tokenmarket login
tokenmarket workspace
tokenmarket connect claude-code
tokenmarket status
```

需要切换空间时，使用 `workspace` 列出的空间 ID：

```bash
tokenmarket workspace <workspace-id>
```

同一机器、同一环境的 CLI 和桌面端共用登录。在任意一端登录后，就能从两端管理连接。切换当前空间会影响新连接；已有工具仍使用接入时绑定的计费空间。

## 支持的工具

桌面端直接选择工具，CLI 命令使用对应的工具 ID：

| 工具 | CLI 工具 ID |
| --- | --- |
| Claude Code | `claude-code` |
| Codex CLI | `codex` |
| WorkBuddy | `workbuddy` |
| DeepSeek Harness | `deepseek-harness` |
| OpenClaw | `openclaw` |
| OpenCode | `opencode` |

Claude Code、Codex 在正式环境使用 `glm-5.3`，测试包使用 `glm-5.2`。其他工具会配置兼容的文本生成模型目录。

## 管理连接

| 操作 | 桌面端 | CLI |
| --- | --- | --- |
| 查看归属、模型和状态 | 打开工具详情 | `tokenmarket status [tool]` |
| 更新连接或更换计费空间 | 切换空间后，点击 **重新接入** | `tokenmarket reconnect <tool>` |
| 恢复接入前的配置 | **取消接入** | `tokenmarket disconnect <tool>` |
| 测试模型请求 | 点击 **测试**，选择模型并确认 | `tokenmarket test <tool> [--model <id>]` |
| 退出两端共用的登录 | 账号菜单 → **退出登录** | `tokenmarket logout` |

接入和重新接入只配置工具，不发送模型请求。**测试会发送一次可能产生费用的模型请求。** 工具配置了多个模型时，通过 `--model <id>` 指定其中一个。

重新接入时，如果账号或计费空间发生变化，会要求确认。取消接入会恢复本次连接接管的字段，遇到后续手动修改时询问处理方式。退出登录会保留工具配置和保存的凭证。

### SSH 与非交互使用

在没有浏览器的机器上运行：

```bash
tokenmarket login --no-browser
```

在其他设备打开输出的授权链接即可，终端会等待确认。浏览器不会获得工具 Key 或长期客户端凭证。

非交互执行时，以下情况需要明确指定选项：

| 场景 | 选项 |
| --- | --- |
| 确认重新接入时更换账号或空间 | `--confirm-rebind` |
| 本地凭证副本全部丢失后恢复连接 | `--confirm-recovery` |
| 取消接入时保留后续修改 | `--conflicts keep-current` |
| 取消接入时恢复原值 | `--conflicts restore-original` |

运行 `tokenmarket <command> --help` 查看各命令选项。

## 凭证与恢复

CLI 和桌面端按环境、账号、空间、设备和工具复用凭证。保存的 Key、连接记录和配置恢复备份均加密存储。macOS、Windows 使用系统凭据库；Linux 优先使用 Secret Service，不可用时使用口令加密库。凭据库锁定或无法读取时会直接报错。

重新接入会校验保存的凭证。已删除或重置的凭证可以补建；禁用或过期时需要到平台处理。余额、网络或权限错误不会生成替代 Key。本地全部副本丢失时，会先要求确认恢复范围，替代配置成功后才处理原凭证。

## 常见问题

| 问题 | 处理方式 |
| --- | --- |
| 找不到已安装的工具 | 桌面端点击 **重新检测**；终端确认工具可从命令行启动，必要时重新打开终端。 |
| 接入后仍使用旧配置 | 完全退出并重开工具；OpenClaw 需重启 Gateway／会话，OpenCode 需检查项目配置是否覆盖全局设置。 |
| 显示“配置已变化”或无法测试 | 按提示修复配置或重新接入，再执行测试。 |
| npm 提示缺少内置二进制 | 重新安装：`npm install -g tokenmarket@1.1.0`。 |
| 需要恢复原配置 | 使用 **取消接入** 或 `disconnect`；加密恢复记录位于 `~/.tokenmarket/desktop-connections/`。 |

### macOS 首次打开被拦截

正式包尚未使用 Apple Developer ID 签名和公证。确认安装包来自本仓库 Releases 后，运行：

```bash
xattr -cr "/Applications/Token Market.app"
```

然后重新打开应用。内测包构建和共享钥匙串签名见 [macOS 内测签名说明](docs/macos-local-test-signing.md)。

## 开发与发布

- [CLI 打包与 npm 发布](docs/cli-npm-release.md)
- [桌面连接管理](docs/desktop-management.md)与[浏览器登录](docs/desktop-login.md)
- [工具检测与配置](docs/tool-inventory.md)
- [共享凭证生命周期](docs/adr/0004-shared-client-key-lifecycle.md)

CLI 和桌面端共用 `rust/tokenmarket-core` 与版本号。发布时，`cli-v1.1.0`、`v1.1.0` 指向同一提交，分别触发 CLI 和桌面端构建。
