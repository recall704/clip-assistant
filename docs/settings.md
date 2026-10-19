# 设置与配置文件

## 1. 文件位置与结构

`~/.clipboard/store/settings.json`（可用 `CLIPASSISTANT_DATA_DIR` 改根目录）。沿用旧版
`electron-store` 的格式，顶层固定三个键：

```json
{
  "settings": { "...": "见下表" },
  "hotKeys": {
    "483278975432": {
      "id": 483278975432,
      "contentType": 0,
      "shortcut": "ctrl+alt+1",
      "name": null
    }
  },
  "database_version": "1"
}
```

- 读取时以 `Settings::default()` 为底，递归覆盖文件里的对象，因此缺字段、旧字段、
  未知字段都不会导致失败。
- 写入是原子的：先写 `settings.json.tmp`，再 `rename` 覆盖。
- 每次写设置都会把整个 `settings` 子树重新序列化，所以 `settings` 内部未知字段会
  被丢弃；顶层其他键（如 `database_version`）保持不变。

## 2. `settings` 字段

枚举值在 JSON 中一律小写。字段名是 camelCase。

### `clipboard`

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `theme` | `light` / `dark` / `system` | `light` | `system` 跟随窗口外观 |
| `layout` | `horizontal` / `vertical` | `horizontal` | 横向停屏幕底部，纵向停右侧 |
| `sounds.open` | bool | `true` | 复制 / 粘贴音效总开关 |
| `sounds.type` | 0 / 1 / 2 | `0` | 0 默认音效，1 小黑子音效，2 自定义 |
| `sounds.userCopy` | `{name, size, url}` | 空 | 自定义“复制”音效，`url` 是本机路径或 http 地址 |
| `sounds.userPaste` | `{name, size, url}` | 空 | 自定义“粘贴”音效 |
| `sounds.defaultCopy` | string | `""` | 兼容旧版保留，当前不参与播放 |
| `sounds.defaultPaste` | string | `""` | 同上 |

音效文件支持 mp3 / wav。http 地址首次播放时下载到 `cache/`；本机路径直接交给 MCI。

### `shortcutKeys`

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `showOrHide` | string | ``ctrl+` `` | 唤出/隐藏面板 |
| `quickPasteEnable` | bool | `true` | 是否注册 `快速粘贴 + 1..9` |
| `quickPaste` | string | `ctrl` | 快速粘贴前缀，只作为字符串与数字拼接 |
| `previousGroup` | string | `ctrl+[` | 上一个分组 |
| `nextGroup` | string | `ctrl+]` | 下一个分组 |

按键写法见 [shortcuts.md](shortcuts.md)。

### `general`

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `autoOpen` | bool | `true` | 开机自启；写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 的 `ClipAssistant` 值 |
| `minTray` | bool | `false` | 启动时最小化到托盘：为真则首次启动不弹设置窗口 |
| `menuIcon` | bool | `true` | 显示托盘图标 |
| `historyCache` | 枚举 | `infinity` | 历史保留窗口，见 [data.md](data.md#6-历史保留) |

自启值写入的是带引号的当前 exe 路径。启动时会顺带清理旧版本留下的 `剪切助手`
值，但仅当它的内容包含 `jianqiezhushou`（避免误删同名条目）。

### 兼容字段

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `cloud.auto` | `false` | 旧版云同步开关，功能已移除，仅保留序列化兼容 |
| `width` | `0` | 旧版窗口宽度 |
| `height` | `0` | 旧版窗口高度 |

## 3. `hotKeys`

“指定内容快捷键”：键是内容 id 的十进制字符串，值：

| 字段 | 说明 |
| --- | --- |
| `id` | 内容 id |
| `contentType` | 创建时的内容类型，设置页用来显示“文本/图片/…” |
| `shortcut` | 按键字符串，如 `ctrl+alt+1` |
| `name` | 可选的显示名，当前写 `null` |

热键槽位由 `id % 1000` 决定（注册 id = `1000 + id % 1000`），因此 id 末三位相同
的两条内容会落在同一个槽位，后注册的覆盖前者。删除内容或清空历史时会同步移除
对应条目。

## 4. `database_version`

旧版用来标记数据库结构版本。当前只在读取时保留、启动时不校验，写设置时不会被
覆盖。新安装通常不存在该键。
