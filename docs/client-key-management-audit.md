# CLI / 桌面 Key 管理重构：集成影响审计

日期：2026-10-30。状态：源码审计完成，方案讨论中。

用户约束：网页、网关、管理平台在接入旧 CLI 前的既有业务语义保持兼容；
允许新增隔离的客户端接口和必要归属字段。此次重构覆盖 Token Market CLI 和
桌面管理入口。已经存在的修改需要逐项归因，不能按时间整批回滚。

本文是本地 Git 历史和当前源码证据，不证明生产部署版本、迁移执行或真实调用结果。
本轮只读审计，没有修改三个被审计仓库、运行测试、访问生产或清理真实 Key。

讨论确认：旧 CLI 目前没有用户，不需要兼容，移除其旧授权/兑换/批量停用整条流程；
新版 CLI 与桌面共享登录、当前空间、凭证和连接管理。以下清单仍保留旧代码现状，
作为删除及回归范围的证据，不代表需要保留旧 CLI 功能。
Q6 已确认增加隔离的签发记录、请求去重和本地持久化确认，以恢复创建响应丢失的情况；
恢复不得提供旧 Key 明文取回，也不得因重试处理已确认的 Key 或其他设备/工具。
Q7–Q10 已确认：本地所有副本丢失需确认替换，新接入成功后精确停用同范围旧客户端 Key；
继续支持 Linux/SSH；CLI 与桌面统一模型配置与显式测试；保留独立的 model_types 和
充值空间名称展示。当前旧 CLI 的自动付费测试及模型预设不进入新流程。
Q11–Q12 已确认：Linux 系统密钥环不可用时允许用户口令加密的后备凭证库，
支持异机浏览器完成远程登录；不做旧 CLI 历史记录迁移，现有桌面安全数据继续复用。

## 源码基线

| 仓库 | 审计时版本 | 旧 CLI 前基线 | 确定性 |
|---|---|---|---|
| llm-platform | bbc / 9bd6a26c，工作树干净 | 082ab353f642d79a4ac54ea62d75b8895878ffe6 | 首次旧 CLI 提交 babd315d 的直接父提交 |
| llm-platform-web | bbc / 1d695e9e，工作树干净 | 2c7a2d161c21f2970c57cd26f28c37dfcd62f8e1 | 首次旧 CLI 提交 16c44c0c 的直接父提交 |
| llm-gateway | main / b13ee2f6，工作树干净 | d7ad53dd7c0dc19670bbebe80abbd6e0cea4b042 | 当前分支在 CLI 开始前的时间锚点，非已验证的部署配套版本 |
| token-market-cli | codex/reuse-desktop-tool-keys / 8a43c8d，加现有工作区修改 | 当前客户端实现单独分析 | 未提交改动包含此前 Key 复用和 Clear Connection 实现，不能当作已发布版本 |

## 已确认的重构去向

| 范围 | 删除或恢复 | 新增或保留 |
|---|---|---|
| Token Market CLI | 删除旧授权兑换调用、旧记录兼容分支、模型预设、setup 自动付费测试 | 与桌面共用会话、空间、凭证、连接、恢复、模型规则；支持 Linux/SSH 和显式测试 |
| 桌面 | 移除 Clear Connection、手动创建/复用 Key 控件和桌面独占的管理逻辑 | 保留正常接入/重接/取消/测试界面，调用共享客户端内核；复用现有安全存储与恢复记录 |
| 平台旧 CLI 域 | 删除专用 setup/authorize/exchange 和批量停用旧 CLI Key 能力，以及专属前后端调用和失效测试 | PKCE、防重放、限流等能力由新的统一管理登录保留，不删除原平台 Key 审计 |
| 平台共享链路 | 恢复原 API-token 空间权限语义，将客户端认证受众限定到专用接口 | 新增隔离的客户端权限/空间/余额门面和 Key 校验、签发关联、保存确认、恢复编排，复用原平台业务服务 |
| 平台 Key 与 Billing | 不修改原加密、摘要、生命周期、鉴权、权限或计费规则 | 保留必要的归属信息和额度初始化；客户端协调状态与 API Token 原状态分开 |
| 网页 | 删除旧 CLI 授权页；客户端回跳/初始空间行为隔离于普通网页默认流程 | 保留原 Key 管理页、专用客户端登录授权入口、安全路径校验与充值空间名称展示 |
| 网关 | 本重构不修改 Key 鉴权、缓存或计费链路 | 保留已有 model_types 公共模型元数据，客户端适配既有字段 |
| 历史数据 | 不做旧 CLI 记录迁移，不按时间整体回滚，不自动删真实本地文件或远程数据库数据 | 已有且归属明确的桌面凭证、配置和恢复历史继续复用；保留无关功能和其他注册客户端隔离 |

这是已确认产品边界的处置清单，不是上述代码已经修改完成的声明。

## 原平台已经存在的规则

- Key 使用 AES-GCM 保护内部字段；保存完整字符串的 SHA-256 摘要和脱敏展示值。
- 创建返回一次明文；普通列表和详情不返回明文或摘要。
- verify、Key 启用/禁用、软删除、重置和旧摘要缓存清理均在上述平台基线前存在。
- resetKey 保持 token_id，生成新密钥、覆盖摘要/脱敏值，清理旧摘要的 Core
  缓存并调用网关 Redis 清理；不会自动恢复状态、额度或历史用量。
- 网页早在 0bd98210（2026-06-26）移除了重置按钮，该提交早于网页 CLI 基线。
  lib/api.ts 保留 resetApiTokenKey 方法，但当前 TokenPanel 没有调用；测试要求不显示按钮。
- baseline→HEAD 的 ApiTokenKeyService、APIKeyUtilsV2、ApiTokenPolicyValidator
  无差异；ApiTokenLogicService 的原生命周期主体未变，变化为归属 metadata 和
  新增的旧 CLI 批量停用能力。

重构不得把这些原平台规则当作客户端新增功能进行撤回或重新定义。

## 管理平台修改清单

| 修改 | 提交 / 当前源码定位 | 影响性质和后续边界 |
|---|---|---|
| CLI setup session、浏览器同意、PKCE、兑换、限流及审计 | babd315d 起；web/cli/controller/CliSetupController.java | 旧 CLI 专用链路按 Q5 删除；新版登录仍需 PKCE、防重放等能力，通过统一客户端会话提供 |
| source/tool/setup_id 字段，后续 device_id；RPC、HTTP DTO、mapper、DDL 扩展 | ec60ea5a、383854dd；ApiTokenLogicService.java:103、:683 | 修改共享数据模型；默认 source=console。新增字段不等于原 Key 行为变化 |
| 同账号＋空间＋工具旧 CLI Key 先批量停用，再创建 | ec60ea5a；CliKeyExchangeService.java:25；ApiTokenLogicService.java:117 | 未按设备隔离，会影响另一台电脑；创建失败时旧 Key 可能已经停用。新流程不应继续使用该替换策略 |
| 共享空间访问 guard 接受范围扩大 | f87448a8；web/auth/service/WorkspaceScopeGuard.java:64 | 原只接受 apiTokenWorkspaces；现在先接受 switchableWorkspaces，再回退前者。影响普通 API-token 管理访问边界，须恢复基线语义或隔离到客户端 guard；下游仍有权限校验，不能宣称所有操作都放开 |
| Desktop Session 登录、同意、兑换、context、renew、logout、持久 grant、注册客户端 | 7cf4afa6、974ef71a、fdaf5eb8、7ce0aba2、309096bf | 新增管理会话，可供新版 CLI / 桌面共用；保持与模型 API Key 的生命周期分离 |
| 全局认证链增加 tm_desktop_ Bearer 分支 | 7cf4afa6、fdaf5eb8；web/base/aspect/AuthAspect.java:463 | 不仅限定 /api/desktop 路径；扩展共享认证接受的凭证类型，下游 endpoint policy 仍生效。应限定新客户端会话的接口受众 |
| 新桌面授权路径的日志省略 | AuthAspect.java:123 | 位于共享切面，主要按新增路径分支；保留必要的凭证保护，不能因此恢复敏感日志 |
| 桌面精确设备工具签发和禁用接口 | 383854dd 起；DesktopConnectionService.java:45、:113 | source=desktop；签发每次新建，无 scope 去重；禁用核对 user/workspace/source/tool/device |
| 签发当前空间全部 active 渠道组，没有组则失败 | DesktopConnectionService.java:65；DesktopWebBuilderUtils.java:16 | 客户端签发编排；沿用原平台允许的空间/渠道组规则，不通过新入口拓宽权限 |
| 原网页创建额度初始化提取为共享 ApiTokenQuotaService | 70c23314；ApiTokenQuotaService.java:17 | 结构重构；前后参数、单位换算和失败语义等价，但确实触碰旧网页调用链 |
| 桌面创建后初始化 Billing KEY quota，失败软删除新 Key | 70c23314；DesktopConnectionService.java:93 | 新客户端必要编排，不能绕过既有 Billing 约束 |
| 既有 /billing/account 响应新增 currency 和 meta 声明 | 58fd9820、448e3d1e；BillingController.java:130 | 既有公共响应扩展，为桌面展示提供币种；须列入兼容审查，不能称平台完全没改 |
| cli_setup_sessions、Key metadata/device_id 数据库迁移 | 6561f246、ec60ea5a、383854dd；docs/sql/temp 下 20261027/20261028 文件 | 源码存在不代表已执行。不得盲目删列、删已存数据或倒退全部 SQL |

同时期的 API 日志缓存 token 等工作与客户端 Key 无关，不能混入回退范围。
已有 Mying 注册客户端支持也不能当作旧 CLI 专属代码整段删除；统一 Token Market
会话时需要保留其他注册客户端的身份、受众与工具范围隔离。

## 网页修改清单

| 修改 | 提交 / 当前源码定位 | 影响性质和后续边界 |
|---|---|---|
| /cli/authorize 及 /api/cli/sessions/* 客户端 | 16c44c0c 起；app/cli/authorize/CliAuthorizeClient.tsx:48；lib/api.ts:7300 | 旧 CLI 专用授权页按 Q5 删除；新版登录与工具 Key 签发分离 |
| 共享登录支持 next 回跳 | 9479b669；app/AppShell.tsx:273、:483、:776；LoginView nextPath | 修改原登录行为，默认流程与专用回跳要分别验证 |
| /desktop/authorize 及专用 API client、注册客户端回跳校验 | f9f2c0d1、e9800847；DesktopAuthorizeClient.tsx:9；lib/api.ts:7289 | 新增专用授权页；同意登录不应创建工具 Key |
| /desktop/console、/desktop/recharge | 54eecd33、1f7cadd4、12968551、83d51d2c；DesktopConsoleClient.tsx:27 | 新增入口，复用原 AppShell/ConsoleShell；涉及浏览器账号和空间校验 |
| 安全 next 路径、初始空间约束、交接后释放约束 | 54eecd33、83d51d2c；AppShell.tsx:241、:251、:295、:427 | 修改共享登录和控制台初始化。应保持普通网页入口的基线语义；保留必要安全校验 |
| 自动展开原充值弹窗 | 54eecd33；ConsoleShell.tsx:22、:65 | 新增 initialRechargeRequested，默认 false；专用入口之外应保持原行为 |
| 充值弹窗显示当前空间名 | 12968551；BillingPanel.tsx:1136、:1822；ConsolePanelRouter.tsx:328 | 影响普通网页的可见 UI；Q10 已确认作为独立功能保留 |
| 原 Key 管理页 | TokenPanel.tsx 及其测试相对基线无差异 | 创建、编辑、启停、删除、明文一次展示并未因 CLI/Desktop 集成改变；重置按钮在基线前已删除 |

## 网关修改清单

- d7ad53dd..b13ee2f6 只涉及 ModelBiz.java、ModelDiscoveryService.java、两个对应测试和一个 E2E shell。
- d8c8088a 给公共模型摘要增加 supported_endpoints；b13ee2f6 改为 model_types，
  从控制面的 modelTypes 取得。集成分支有同类历史提交 1fca0e59 和合并点 7f501011，
  不应误认为它们均属于当前 main 的直接谱系。
- ModelBiz.java:156 和 ModelDiscoveryService.java:125、:265 的变化影响公共
  OpenAI / Anthropic / Gemini 模型目录输出，不是仅客户端内部字段。
- 此模型元数据也有独立的已讨论产品背景，不能仅按发生时间判定应回退。
  Q10 已确认保留。新客户端应适配平台既有公开契约，不为自身另建推断字段。
- Access 鉴权源码相对基线无差异：未因集成改变 SHA-256、Key 状态/过期、缓存或计费入口。
- be00cf42 是 E2E shell 参数修正，不是运行时 Key 行为。

## 当前 CLI 与桌面的差异

| 环节 | 旧 CLI 当前代码 | 桌面当前工作区代码 | 已确认的新方向 |
|---|---|---|---|
| 登录 | setup 每次浏览器授权并兑换工具 Key | 可复用 Desktop Session | 共用 Client Session 和 Current Workspace |
| Key 复用 | setup 不读取桌面复用库 | 按环境/账号/空间/设备/工具查加密缓存 | 两个入口共用同一份凭证与连接记录 |
| 本地历史 | 根 keys.json 的 ManagedKeyRecord，缺少账号/空间字段，按工具替换元数据 | desktop-connections 下凭证、记录、字段恢复信息 | 需要可信迁移规则，不能猜旧记录归属 |
| 失效处理 | 创建新 Key；服务端旧流程会先批量停用 | 缓存命中后请求 /v1/models，失败保留原配置并报错 | 专用校验区分有效/删除/重置/禁用/过期/失败 |
| 取消接入 | 当前 CLI 无统一 Disconnect 命令 | 恢复接管字段，保留凭证；另有待移除的 Clear Connection | 共用恢复内核，普通取消保留 Key，移除清除连接 |
| 测试 | setup 自动执行模型请求，可能计费 | 另由用户显式启动测试 | 新 CLI 的测试与模型交互政策仍待决定 |

README.zh-CN.md 当前写“重新运行 setup 不会自动撤销旧 Key”，但旧 CLI 实际
仍调用会批量停用旧 CLI Key 的兑换接口。文档与执行路径不一致，须在最终迁移时一并修正。

## 实现前必须解决的边界

- 校验必须结合当前会话的账号、空间、Key 归属和状态，不能只有 exists 布尔值。
  软删除后摘要仍可能查到；重置后记录 ID 仍在但摘要变化。
- 余额/额度/模型权限错误不等于 Key 身份消失，不能触发重新签发。
- 当前公共 createToken 不提供设备工具唯一约束。减少重复需要客户端专用编排，
  不能把公共 createToken 改为返回无法恢复明文的旧 Key。
- 创建成功但响应丢失、已确认本地密钥丢失、旧 CLI 无归属元数据迁移、同机两入口并发
  都要在 ADR 中作出决策，不能只测试普通重连路径。
- 原平台共享逻辑的兼容回归、客户端独立协议测试、本地迁移与打包/钥匙串验收应分别证明。

设计讨论见 [ADR 0004](adr/0004-shared-client-key-lifecycle.md)。
