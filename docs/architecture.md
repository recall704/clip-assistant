# 架构

## 1. 一个进程，两个 crate

整个应用是单个 `clipassistant.exe`（`#![windows_subsystem = "windows"]`，不弹控制台）。
界面用 GPUI 0.2（Zed 的 GPU 加速 UI 框架），其余全部是进程内 Win32 调用。

```
clip-assistant/
├── crates/core/   clipassistant-core   平台与业务逻辑，无 UI 依赖，可无显示环境测试
└── crates/app/    clipassistant-app    GPUI 界面 + 进程装配
```

| crate | 模块 | 职责 |
| --- | --- | --- |
| core | `model` | `Post` / `PostGroup` / `PostIcon` / `ContentType` / `PostQuery` / `FileDescriptor`，字段名与旧版 SQLite / TypeScript 类型一致 |
| core | `db` | SQLite 打开、建表、增删改查、软删除、历史清理 |
| core | `settings` | `settings.json` 读写、默认值合并、热键映射、分组新建负载 |
| core | `clipboard` | 剪贴板监听（`WM_CLIPBOARDUPDATE`）、读取五类内容、写入文本/HTML/文件 |
| core | `hotkey` | 全局热键注册/注销、按键字符串解析 |
| core | `paste` | 构造可粘贴内容、写剪贴板、恢复前台窗口、合成 `Ctrl+V` |
| core | `layout` | 光标所在显示器边界/缩放、面板停靠几何 |
| core | `sound` | 运行时合成提示音、MCI 播放自定义音频 |
| core | `autostart` | `HKCU\...\Run` 开机自启 |
| core | `source_app` | 抓取前台进程名与可执行文件图标 |
| core | `icon` | 从 exe 提取图标、按 SHA-256 缓存 PNG |
| core | `winutil` | 消息窗口、前台窗口、`SendInput`、路径大小、Shell 打开 |
| core | `api` | 旧后端客户端（更新公告、链接信息、上传、展示位），全部失败即忽略 |
| core | `util` | 15 位 ID、时间格式化、链接判断、data URL、HTML 转文本等 |
| app | `main` | 进程装配、事件泵、托盘事件分发、更新公告拉取 |
| app | `state` | 全局 `AppState`：DB、Store、热键管理器、指定快捷键、采集/粘贴/清理 |
| app | `windows` | 窗口注册表：面板、设置、更新弹窗的打开/关闭/定位 |
| app | `actions` | 设置变更落地：热键重注册、主题、布局、音效、自启、热键分发 |
| app | `tray` | 托盘图标与右键菜单（独立线程） |
| app | `ui/*` | 面板、卡片、设置页、自绘文本输入框、主题、更新弹窗、代码高亮 |

## 2. 启动顺序

`crates/app/src/main.rs`：

1. `env_logger` 初始化，默认过滤级别 `info`（`RUST_LOG` 可覆盖）。
2. `AppState::new()`：打开 `~/.clipboard/store/settings.json` 和 `~/.clipboard/db.sqlite`，
   读取 `hotKeys` 到内存；失败则打印并退出。
3. 进入 GPUI `Application::run`：
   1. 注册全局 `AppState`、窗口 `Registry`，绑定文本输入与面板按键；
   2. 创建一个不可见的 `KeepAlive` 窗口，避免关闭面板后 GPUI 退出事件循环；
   3. 创建 `HotkeyManager`，按设置注册全部热键；
   4. 启动 `ClipboardMonitor`（独立消息窗口 + 转发线程）；
   5. 同步开机自启状态，执行一次历史清理；
   6. 启动托盘线程，按 `general.menuIcon` 决定是否显示图标；
   7. 启动 40ms 事件泵；
   8. 处理调试环境变量；
   9. 首次启动时打开设置窗口——除非 `general.minTray` 为真或设置了
      `CLIPASSISTANT_SKIP_SETTINGS`。

## 3. 线程与消息

| 线程/任务 | 所属 | 作用 |
| --- | --- | --- |
| GPUI 主线程 | app | 所有窗口、实体、绘制与大部分状态变更 |
| 背景事件泵 | app | 每 40ms 汇总热键队列、剪贴板计数、托盘事件、失焦检测 |
| 热键线程 | core::hotkey | 消息窗口接收 `WM_HOTKEY`；注册命令通过 `WM_APP_INVOKE` 唤醒后执行 |
| 剪贴板监听线程 | core::clipboard | 消息窗口收 `WM_CLIPBOARDUPDATE`，通过 channel 交给转发线程调用回调 |
| 粘贴线程 | core::paste | 恢复前台窗口、等待 100ms、`SendInput` 合成 `Ctrl+V` |
| 托盘线程 | app::tray | 独立消息循环，菜单命令写入全局事件队列 |
| 临时文件清理 | core::paste | 图片粘贴后 10 秒删除临时 PNG |
| 音效播放 | core::sound | 内置音调用 `PlaySound`，自定义文件用 MCI 播放，5 秒后关闭别名 |

热键回调不直接操作 UI，只把热键 id 推入队列；剪贴板回调只累加原子计数。真正的
UI 工作都在事件泵里通过 `cx.defer` 投递，避免从 GPUI 实体更新内部重入（会触发
实体租约 panic）。窗口的置前/聚焦也刻意走纯 Win32（`SetWindowPos` +
`SetForegroundWindow` + `SetFocus`），不经过 `Window::activate_window()`。

## 4. 事件泵

`cx.spawn` 循环，每 40ms：

1. 取出并清空热键 id 列表；
2. `swap` 剪贴板计数为 0；
3. 取出托盘事件；
4. 若面板已打开且已过 400ms 聚焦稳定期却不再拥有前台，关闭面板；
5. 对每个托盘事件、热键 id 调用 `cx.defer`；
6. 对每次剪贴板变化调用 `AppState::capture_clipboard()`；面板开着则刷新列表；
7. 连续 1500 次空闲（约 60 秒）执行一次历史清理。

## 5. 窗口

窗口句柄放在全局 `windows::Registry`（`Mutex<Windows>`）里。`WindowHandle` 不是
`Copy`，所以所有访问都是“取出 → 使用 → 放回”。

| 窗口 | 尺寸与位置 | 特性 |
| --- | --- | --- |
| 剪贴板面板 | 横向：全宽 × 340；纵向：290 × 全高；停靠在光标所在显示器 | `PopUp`、透明背景、不可缩放/移动、无标题栏 |
| 设置窗口 | 760 × 440，贴近点击处 | 启动时居中；从托盘或面板菜单打开时靠近按钮；单实例，再次打开只激活 |
| 更新弹窗 | 320 × 320，居中 | `PopUp`，同时间只开一个 |
| KeepAlive | 不可见 | 保持进程与事件循环存活 |

面板显示流程：按当前布局计算逻辑像素矩形（物理像素 ÷ 显示器缩放）→ 记录当前前台
窗口 → 创建窗口并记住 HWND → 本轮更新结束后置前聚焦 → 180ms 后再补一次聚焦
（新窗口第一次激活可能被系统丢弃）。面板失焦不会抢回焦点，而是关闭自己。

## 6. 关键数据流

### 采集剪贴板

```
WM_CLIPBOARDUPDATE
  → 监听线程回调（原子计数 +1）
  → 事件泵
  → clipboard::read_payload()          图片 > 文件 > 文本
  → source_app::capture_json()         前台进程名 + 图标
  → clipboard::build_post()            生成 15 位 id 的 Post
  → 按 content 查重（status = 0）
       命中：只刷新 editTime，不新增
       未命中：插入、播放复制音效
  → 面板打开时刷新列表
```

### 粘贴

```
面板双击 / 回车 / 快速粘贴 / 指定快捷键
  → AppState::paste_post()             按设置播放粘贴音效
  → paste::printable_for()             文本+HTML / 临时图片文件 / 文件路径列表
  → 写系统剪贴板
  → 恢复面板打开前记录的前台窗口
  → 100ms 后 SendInput(Ctrl+V)
  → touch_post()                       刷新 editTime，下次排到最前
```

### 全局热键

```
RegisterHotKey → WM_HOTKEY → HotkeyManager 回调 → 队列
  → 事件泵 → actions::on_hotkey(id)
      1 显示/隐藏面板
      2/3 上一个/下一个分组
      101..109 快速粘贴第 1..9 条
      ≥1000 指定内容快捷键
```

## 7. 与 Electron 原实现的对应

| 原实现 | 现实现 |
| --- | --- |
| `electron-store` | `settings::Store`（同一份 `settings.json`、同样的顶层键） |
| Sequelize + SQLite | `db::Db`（同样的表名/列名/时间格式） |
| `clipboardListener.exe` | `clipboard::ClipboardMonitor`（`AddClipboardFormatListener`） |
| `electron.clipboard` | `clipboard::{read_payload, write_text, write_text_and_html, write_files}` |
| `globalShortcut` | `hotkey::HotkeyManager`（`RegisterHotKey`） |
| `robotjs.keyTap` + `setHandle.exe` | `paste` + `winutil::{set_foreground_window, send_ctrl_v}` |
| `getHandle.exe` | `winutil::foreground_window` |
| `getFolder.exe` | `winutil::path_size` |
| `Tray` + `Menu` | `app::tray`（运行时绘制图标，Win32 弹出菜单） |
| `active-win` + 云端图标 | `source_app` + `icon`（本机提取图标，无网络依赖） |
| renderer 的 `/clipboard`、`/settings` 路由 | `ui::panel::PanelView`、`ui::settings::SettingsView` |
