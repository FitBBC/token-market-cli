# macOS 内测包共享钥匙串签名

CLI 和桌面是同一个 Token Market Client 的两个入口。macOS 内测包使用
**固定自签名证书、相同的 designated requirement（DR）**，让两端都满足
新建钥匙串记录的可信程序规则。无需 Apple Developer 账号，凭证仍存放在
系统钥匙串，工具 Key、连接和恢复记录继续使用原有加密存储。

这个方案只用于内部测试，不提供 Developer ID、公证或 Gatekeeper 信任。
下载后首次打开应用的系统信任提示与钥匙串访问弹窗是两类不同问题。

## 构建 macOS ARM 测试包

```bash
bash scripts/build-test-clients.sh
```

需要 Rust、Python 3、Xcode Command Line Tools 及 Tauri CLI。如果没有安装
`cargo tauri`，可以通过 `TAURI_CLI_JS` 指向已安装的 `@tauri-apps/cli/tauri.js`。
可传第一个参数指定空的输出目录。此脚本先构建测试环境 CLI 和 App，再做
钥匙串互读测试、签名校验，最后生成 CLI、App、DMG 和签名摘要。
单独的 `build-test-desktop.sh` 是底层构建步骤；要交付共享签名测试包，使用
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

共享 DR 同时检查证书指纹，并且只接受以下两个签名标识：

- `cn.tokenmarket.setup.test`：桌面测试 App。
- `cn.tokenmarket.setup.cli.test`：CLI 测试二进制。

这不是“允许所有应用访问”。临时签名程序、其他证书签名程序，以及同一
证书签名但标识不同的程序不满足规则。两端声明相同的 DR，且使用同一张
长期保留的证书；仅更改文件名、目录或重新编译不会改变这套身份。
测试和生产的会话仍按平台 URL 隔离，签名不改变凭证、Key、权限和计费协议。

打包只接受测试 App 标识，保留原签名中的 entitlements、flags、runtime，
不添加共享访问组权限，不需要 provisioning profile。生成的签名摘要记录
公开证书指纹、共享规则和 CLI/DMG SHA-256，不包含私钥或任何登录凭证。

## 已构建产物重新签名

如果两个产物已是同一次测试环境源码构建，可直接复制、重新签名和制作 DMG。
原始产物保持不变，输出目录必须为空：

```bash
python3 scripts/macos-local-signing.py init
python3 scripts/macos-local-signing.py smoke
python3 scripts/macos-local-signing.py package \
  --app 'src-tauri/target/release/bundle/macos/Token Market Test.app' \
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
升级时可能仍出现一次迁移授权；CLI 和桌面可能各需授权一次。不会静默
删除旧会话、加密主密钥、工具 Key 或恢复记录来掩盖弹窗，也不会重发 Key。
全新记录由这套共享签名身份创建后，两端正常互读不需要该授权弹窗。

## 依据

- [Apple：Sharing a designated requirement](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html)：允许协同程序使用相同 DR 共享钥匙串访问。
- [Apple：Understanding the Code Signature](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/AboutCS/AboutCS.html)：钥匙串支持自签名身份，但自签名不提供 Gatekeeper 分发信任。
- [Apple：Access Control Lists](https://developer.apple.com/documentation/security/access-control-lists)：可信应用满足 ACL 时直接访问，否则需要用户确认。
