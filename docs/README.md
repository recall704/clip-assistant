# 剪贴助手文档

本目录按主题记录当前 Rust + GPUI 实现的真实行为。文档以代码为准：`crates/core`
是无 UI 的平台/业务层，`crates/app` 是 GPUI 界面层。

| 文档 | 内容 |
| --- | --- |
| [architecture.md](architecture.md) | 进程结构、线程模型、启动顺序、事件泵、窗口管理、关键数据流 |
| [data.md](data.md) | 数据目录、SQLite 表结构、`Post.content` 各类型格式、去重与历史保留 |
| [settings.md](settings.md) | `settings.json` 全部字段、默认值、`hotKeys` 映射与兼容性 |
| [ui.md](ui.md) | 剪贴板面板、卡片、右键菜单、设置窗口、更新弹窗、托盘、主题色 |
| [shortcuts.md](shortcuts.md) | 全局热键、面板内快捷键、快速粘贴、指定内容快捷键、按键写法 |
| [development.md](development.md) | 构建、测试、调试环境变量、图标生成、已知限制与排查 |

仓库根目录的 [README.md](../README.md) 是面向使用者的快速开始。
