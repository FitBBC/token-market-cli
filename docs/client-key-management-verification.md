# CLI／桌面 Key 重构验收记录

日期：2026-10-30。依据用户确认的 ADR 0004 完成源码重构及测试包。

## 实现结果

- CLI、桌面共用按环境隔离的登录、当前空间、设备身份、加密 Key 缓存和配置恢复记录。
- 按环境＋账号＋空间＋设备＋工具复用 Key；重新接入先验证真实记录、摘要、归属、状态和有效期。
- 删除／明确摘要变化才允许补建；禁用／过期提示平台处理。网络、会话、权限、余额、额度和模型错误均不会补建。
- 新发放先加密保存，再确认保存、读取目录和写配置。配置提交前再次校验服务器状态和共享登录。
- 初次响应丢失仅轮换未确认记录的摘要，不增加 Key 行；代次阻止迟到确认。保存确认丢失重用已保存的秘密。
- 所有本地副本丢失时先尝试从真实工具配置恢复；仍缺失才确认具体范围并补建。成功写配置后才处理同范围前序记录，连续中断也不会遗忘前序记录。
- 普通取消接入只恢复接管字段并保护后续修改，保留凭证。移除清除连接、强制创建及旧 CLI 授权／历史读取／批量停用流程。既有数据库历史列不作破坏性删除。
- Linux 优先 Secret Service，允许口令加密后备库；存储错误不当作不存在。远程浏览器只确认登录，不获得工具 Key 或长期会话。
- 原网页 Key 生成、重置、鉴权、权限和计费规则保持；网关未改。恢复原 API-token 空间权限边界，客户端新增能力走隔离接口。

## 源码位置

| 仓库 | 工作树 | 分支／基点 |
| --- | --- | --- |
| CLI／桌面 | /Users/bingbing.chen/mying/token-market-cli | codex/reuse-desktop-tool-keys / 8a43c8d，包含本次及此前未提交修改 |
| 管理平台 | /private/tmp/tokenmarket-key-refactor-platform | codex/shared-client-key-management / 9bd6a26c |
| 网页 | /private/tmp/tokenmarket-key-refactor-web | temp/shared-client-key-management / 1d695e9e |

原管理平台和网页工作树仍干净；未提交、推送、合并或部署。
完整历史影响范围见 client-key-management-audit.md；正式决策见 ADR 0004。

## 自动化结果

| 检查 | 结果 | 证据范围 |
| --- | --- | --- |
| Rust core | 124 通过 | 包含新协议直接 HTTP 测试、响应丢失、存储失败、禁用／过期、删除／重置、恢复及另一入口退出时的提交保护 |
| 桌面界面 | 11 通过 | 真实 Chrome＋受控 IPC 夹具；不是已安装应用的原生验收 |
| npm | 13 通过 | 启动和打包脚本 |
| CLI／Tauri | 编译与测试包成功 | offline／locked；CLI help/version 可运行 |
| 网页 | 全部 2045 通过 | 包含原 Mying 回跳及新增远程确认 |
| 网页生产源码类型 | 通过 | 原全仓检查的测试类型错误另列，没有修改检查配置 |
| 网页 lint／KB | 通过 | 知识库与源码一致 |
| 服务端专项 | core 32、webapi 39，全部通过 | 新协议、缓存事务时序、原 Key 服务、客户端受众和空间权限 |
| App 签名／DMG 完整性 | 通过 | ad-hoc 测试 App；未安装或启动 |
| 三个工作树 diff --check | 通过 | 无空白错误 |

全量服务端回归：core 1156 项中有 1 项既有失败、12 项跳过；webapi 686 项中有 2 项既有失败。
已在未修改的原平台单独复现同样失败：

- WorkspaceRpmConfigLogicServiceTest.oldRpmOnlyStoredRulesRemainReadableInSnapshotAndAdminList
- AdminRememberTokenOptimizationHttpTest.tc113_should_reject_blank_audit_reason
- MetaSchemaRegistryTest.creditsMetadataDoesNotDependOnCurrency

网页全仓 TypeScript 检查的 46 行既有测试诊断，除工作树绝对路径外与未修改基线完全一致；本次生产源码类型检查通过。
没有为通过检查而更改无关实现或屏蔽测试。

## 测试包

- dist/shared-client-key-20261030/TokenMarket-Test-1.0.0-arm64.dmg
- dist/shared-client-key-20261030/tokenmarket-test-0.1.0-macos-arm64
- dist/shared-client-key-20261030/manifest.json：构建输入 SHA256、源码基点、测试环境默认值及产物指纹。

App 标识 cn.tokensmarket.setup.test；默认 WebAPI https://console-api-test.tokensapi.cn，
Gateway https://api-test.tokensapi.cn，必需模型 glm-5.2。运行时显式环境变量仍可覆盖构建默认值。

## 部署与真实验收边界

尚未执行 SQL 或部署服务。先在测试环境 **user writer 数据库**执行
/private/tmp/tokenmarket-key-refactor-platform/docs/sql/temp/20261030_client_key_bindings.sql，
再部署匹配的 core／webapi 和网页授权页，最后使用新 CLI／桌面包。
现有公共接口无需改变；旧 CLI 专用入口删除。新契约记在平台 docs/page-schema.md。

以下尚未验收，不能由单元测试或编译成功代替：

- 真实 PostgreSQL 并发／故障注入及迁移执行。
- 已安装桌面应用、系统钥匙串交互和真实浏览器授权回跳。
- 真实 Linux Secret Service／SSH 主机：本机已覆盖口令库加密测试并编译 Linux 实现分支，未在 Linux 上运行。
- Windows 原生运行及真实付费模型请求。

本次没有安装应用、主动变更 tokenmarket:// 默认接收应用、操作真实凭证或发起付费模型请求。
