# 剪贴助手 · ClipAssistant

Windows 上的剪贴板历史工具：热键唤出面板，`Ctrl+1..9` 直接粘贴，支持文本 / 图片 /
链接 / 文件 / 文件夹五类内容，分组管理、搜索、粘贴后自动置顶、托盘常驻。数据全部保存在
本地，离线可用。

整个应用是一个 Rust 进程：界面用 [GPUI](https://gpui.rs/)（Zed 的 GPU 加速 UI 框架），
剪贴板监听 / 读写、全局热键、粘贴、托盘、音效都用 Win32 API 在进程内完成，不再需要
Electron 时代的辅助 exe。

## 快速开始

```bash
cargo build --release
target/release/clipassistant.exe
```

- 首次运行会打开设置窗口（`general.minTray` 为假时）。
- 默认热键 ``Ctrl + ` `` 唤出 / 隐藏面板；切换布局后按光标所在显示器停靠。
- 面板内：方向键选择，回车粘贴，`Ctrl+F` 搜索，`Ctrl+[` / `Ctrl+]` 换分组，
  `Ctrl+1..9` 快速粘贴全部历史的前 9 条，直接打字即搜索，Esc 关闭。
- 托盘图标左键切换面板，右键打开菜单。

## 数据位置

| 内容 | 路径 |
| --- | --- |
| SQLite 历史 | `~/.clipboard/db.sqlite` |
| 设置（JSON） | `~/.clipboard/store/settings.json` |
| 图标缓存 | `~/.clipboard/icons/` |

调试时设置 `CLIPASSISTANT_DATA_DIR` 指向别的目录，避免碰到真实数据。

## 测试

```bash
cargo test    # 32 个单元测试：26 core + 6 app
```

`core` 不依赖 GPUI，测试不需要显示环境。

## 调试开关

| 变量 | 作用 |
| --- | --- |
| `CLIPASSISTANT_DATA_DIR` | 使用独立的数据目录 |
| `CLIPASSISTANT_SHOW_PANEL=1` | 启动即显示面板 |
| `CLIPASSISTANT_SKIP_SETTINGS=1` | 启动时不弹设置窗口 |
| `CLIPASSISTANT_FAKE_HOTKEY=1` | 1.5 秒后模拟一次显隐热键 |
| `RUST_LOG` | 日志级别，默认 `info` |

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | 进程/线程模型、启动顺序、事件泵、窗口管理、数据流 |
| [docs/data.md](docs/data.md) | 数据目录、SQLite 表结构、内容格式、去重与保留策略 |
| [docs/settings.md](docs/settings.md) | `settings.json` 全部字段与默认值 |
| [docs/ui.md](docs/ui.md) | 面板、卡片、设置页、托盘、主题 |
| [docs/shortcuts.md](docs/shortcuts.md) | 全局热键与界面内快捷键 |
| [docs/development.md](docs/development.md) | 构建、测试、调试、目录结构、已知限制 |

## 结构

```
clip-assistant/
├── crates/
│   ├── core/    平台与业务逻辑（无 UI，可单元测试）
│   └── app/     GPUI 界面与进程装配
├── docs/        设计与行为文档
├── tools/       icon-gen：生成 assets/icon.ico
└── assets/
```
