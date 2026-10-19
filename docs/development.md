# 开发

## 1. 环境与构建

- Windows 10/11，Rust stable（edition 2021），MSVC 工具链。
- 唯一的 UI 依赖是 GPUI 0.2.2；`core` 不依赖 GPUI。

```bash
cargo build            # 开发构建（opt-level 1，依赖 opt-level 2）
cargo build --release  # 发布构建（thin LTO，单 codegen unit）
target/release/clipassistant.exe
```

根 `Cargo.toml` 是 workspace，成员为 `crates/core`、`crates/app`；`tools/icon-gen`
被排除在 workspace 之外，单独构建。`tools/icon-gen/target` 与根 `target` 都在
`.gitignore` 中。

发布版可执行文件是 `target/release/clipassistant.exe`（bin 名在
`crates/app/Cargo.toml` 中定义为 `clipassistant`）。

## 2. 测试

```bash
cargo test
```

当前共 32 个单元测试：

| 位置 | 数量 | 覆盖 |
| --- | --- | --- |
| `crates/core` | 26 | 建表/查询/排序/软删除/分组、设置往返与默认合并、热键解析、布局停靠、链接与大小格式化、data URL、图标缓存、API 信封 |
| `crates/app` | 6 | 代码高亮分词、主题解析与分组配色 |

`core` 的测试不创建窗口，可在无显示环境运行；`app` 的 6 个测试只测纯函数。
数据库测试使用内存库，设置测试使用临时目录。

## 3. 调试环境变量

| 变量 | 作用 |
| --- | --- |
| `CLIPASSISTANT_DATA_DIR` | 指定数据根目录，避免碰到真实历史与设置 |
| `CLIPASSISTANT_SHOW_PANEL=1` | 启动即显示面板，不按热键 |
| `CLIPASSISTANT_SKIP_SETTINGS=1` | 启动时不弹设置窗口 |
| `CLIPASSISTANT_FAKE_HOTKEY=1` | 1.5 秒后向热键队列投递一次“显隐”，走真实热键分支 |
| `RUST_LOG` | 日志级别，默认 `info`；如 `RUST_LOG=debug` 可看到面板几何、置前结果等 |

程序是 `windows_subsystem = "windows"`，不会自己开控制台；从终端启动并重定向
stderr 才能收日志：

```powershell
$env:RUST_LOG = "debug"
$env:CLIPASSISTANT_DATA_DIR = "$env:TEMP\clipassistant-test"
target\debug\clipassistant.exe 2> clipassistant.log
```

## 4. 图标生成

`assets/icon.ico` 由独立工具生成，包含 16/20/24/32/40/48/64/128/256 九个尺寸，
128 及以上用 PNG 条目：

```bash
cargo run --manifest-path tools/icon-gen/Cargo.toml -- assets/icon.ico
```

图形与托盘图标同款：主题色圆角方块 + 白色剪贴板。托盘图标不读这个文件，而是在
`app::tray` 里用 GDI 现场绘制；`icon.ico` 供可执行文件/任务栏等资源使用。

## 5. 目录结构

```
clip-assistant/
├── Cargo.toml / Cargo.lock
├── README.md
├── assets/icon.ico
├── docs/                     本文档
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── model.rs      Post / PostGroup / ContentType / PostQuery
│   │       ├── db.rs         SQLite
│   │       ├── settings.rs   settings.json
│   │       ├── clipboard.rs  剪贴板读写与监听
│   │       ├── hotkey.rs     全局热键
│   │       ├── paste.rs      粘贴流程
│   │       ├── layout.rs     面板几何
│   │       ├── sound.rs      音效
│   │       ├── autostart.rs  开机自启
│   │       ├── source_app.rs 来源应用
│   │       ├── icon.rs       exe 图标缓存
│   │       ├── api.rs        旧后端客户端
│   │       ├── util.rs       通用工具
│   │       └── winutil.rs    Win32 辅助
│   └── app/
│       └── src/
│           ├── main.rs       启动与事件泵
│           ├── state.rs      全局状态与业务动作
│           ├── windows.rs    窗口注册表
│           ├── actions.rs    设置应用与热键分发
│           ├── tray.rs       托盘
│           └── ui/           panel / card / settings / text_input / theme / widgets / update / highlight
└── tools/icon-gen/           图标生成工具
```

## 6. 改代码时的注意点

- **不要从 GPUI 实体更新里重入实体。** 面板的搜索回调、事件泵里的窗口操作都通过
  `cx.defer` 延后；窗口聚焦走纯 Win32，不调用 `activate_window()`。
- **窗口句柄要从 `Registry` 取出再放回。** `WindowHandle` 不是 `Copy`，`update_panel`
  是标准写法。
- **热键注册命令走消息线程。** 在 `HotkeyManager` 上调用 `register` 只是入队 +
  `PostMessage`，真正注册发生在窗口过程里。
- **数据格式保持兼容。** 表名、列名、`settings.json` 顶层键、`Post` 字段名都对应
  旧版；改字段需要同时考虑旧库读取。
- **设置写入是整棵子树覆盖。** 新增字段要给 `#[serde(default)]`，否则旧文件读不出。

## 7. 已知限制

以下行为不符合直觉或尚未完成，改动前先确认是否需要修：

1. **设置页只有三个**：剪贴板 / 快捷键 / 通用设置。源码注释里提到的“同步”页和
   `cloud` 配置都随云功能移除了。
2. **多文件剪贴板只取第一个路径。** `build_post` 使用 `paths.first()`，因此复制多个
   文件时历史里只有一项。
3. **图片粘贴是“临时文件”方式。** 图片会写成 `tmp/*.png` 后以 `CF_HDROP` 放入
   剪贴板，10 秒后删除；不是 `CF_DIB` 位图。
4. **面板最多 200 条。** 列表不做分页，超过 200 条的部分当前不可见。
5. **指定快捷键可能撞槽位。** 注册 id 为 `1000 + id % 1000`，末尾三位相同的内容会
   互相覆盖。
6. **设置页修改指定快捷键不落盘。** `SettingsView::apply_appoint` 目前只触发重注册，
   没有写入 `hotKeys`；真正保存的是面板右键 → “指定快捷键”。
7. **API 大多处于休眠状态。** `report_*`、`link_info`、`upload_base64`、`display_all`
   只有客户端实现，界面目前只调用 `display/get?biz=updateAnnouncement`。所有请求
   5 秒超时且失败即忽略。
8. **去重是全局的。** 只要 `status = 0` 的记录内容相同就只刷新时间，不区分来源应用
   或分组。
9. **没有单实例锁。** 同时启动两个进程会争抢全局热键，后启动的注册失败。
