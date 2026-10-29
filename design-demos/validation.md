# 验证与边界

2026-10-29，使用 Playwright Node 运行系统 Chrome 的真实浏览器渲染。先用当前 UI + 公开演示夹具进行对照，中文、Logo、SVG、原生控件渲染正常。

## 已完成
- 三方向分别检查 1440×1000、900×760、375×812，共 9 个视口。截图在 screenshots/。
- 检查页面异常、图片完整性、横向溢出、文字大小、对比度、点击目标。首轮发现的浅色字、12px说明正文、34/42px点击控件已修正。
- 三版初始视图：无 JS 异常，无裂图，无横向溢出，无小字/44px控件告警。对比度检查有 1 个滚动裁剪误报：B 在900宽度下，被详情滚动区裁掉的 Anthropic Messages 标签被错误地和其后的操作按钮比较；滚入视野后重新检测无此告警，见 scroll-validation.json 与 dir-2-900-scrolled.png。机器原始结果保留在 validation.json。
- 浏览器实际点击：A 9 项、B 10 项、C 8 项。覆盖搜索、筛选、切换当前空间但保留已有绑定、新接入使用新空间、测试前费用确认、配置变化禁用测试、登录演示返回。B 在1440和900宽度下主按钮在首屏可见。机器结果 interaction-validation.json。
- A/B 实际切换失败结果；C 的状态演示支持失败/配置变化。测试确认框截图 dir-*-test-dialog.png。
- 所有 Logo/图标内嵌 data URI，不依赖远程字体/CDN。
- 三个骨架肉眼对比：侧栏卡片库 / 分栏控制台 / 顶导航连接图＋列表；不是同结构换色。

## 工具适配
skill 的 Python design_lint.py check/compare/ledger check 已尝试，因 Python Playwright 未安装跳过。使用同一 design_lint.py 的 COLLECT_JS 采集逻辑，通过现有 Node Playwright 执行尺寸/对比度检查；截图人工逐张检视。未把 Python 的跳过结果计为通过。

## 边界
仅 UI 原型，所有数据与操作均为本地模拟。未真实登录、读账号/本机凭据、调用模型、计费、启动/安装工具，也未执行 Rust/业务测试。未修改现有 src、Rust 或已有工作区改动。按三方向门，等待用户选择后再展开完整 UI 与接入实现。当前无已获批方向，不写虚假的 direction-approved 记录。

## 来源
- 当前仓库 src/tauri-ui 与 docs/desktop-management.md、docs/tool-inventory.md。
- Token Market 官网 https://www.tokensapi.cn/，品牌图标沿用 src-tauri/icons/128x128.png。
- 参考 https://manual.raycast.com/extensions、https://culturedcode.com/things/，实际 UI 截图见 refs/。
