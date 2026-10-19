# 数据与存储

## 1. 数据目录

默认根目录为 `~/.clipboard`，可用环境变量 `CLIPASSISTANT_DATA_DIR` 覆盖（空字符串
视为未设置）。根目录由 `clipassistant_core::data_dir()` 统一给出。

| 路径 | 内容 |
| --- | --- |
| `db.sqlite` | 剪贴板历史、分组、图标索引 |
| `store/settings.json` | 设置、指定内容快捷键、数据库版本 |
| `tmp/` | 粘贴图片时写出的临时 PNG（10 秒后删除） |
| `icons/` | 从可执行文件提取的来源应用图标，文件名为 SHA-256 |
| `cache/` | 下载的远程资源（自定义音效等） |
| `sounds/` | 预留的自定义音效缓存目录 |

SQLite 以 WAL 模式打开（`journal_mode = WAL`），目录不存在时自动创建。

## 2. SQLite 表

建表使用 `CREATE TABLE IF NOT EXISTS`，因此已有安装的旧库可以直接继续使用。
时间列是本地时间的 `YYYY-MM-DD HH:mm:ss` 字符串，不是 Unix 时间戳。

### `post`

| 列 | 类型 | 说明 |
| --- | --- | --- |
| `id` | BIGINT PK | 15 位随机数字，`util::magic_id()` |
| `title` | VARCHAR(255) | 卡片标题；空时界面回退为类型默认名 |
| `content` | TEXT | 原始负载，格式见第 3 节 |
| `description` | TEXT | 新记录固定写 `暂无` |
| `contentType` | TINYINT | 0 文本、1 图片、2 链接、3 文件、4 文件夹 |
| `sourceApp` | VARCHAR(255) | JSON：`{ name, icon, platform, hash }` |
| `tags` | VARCHAR(255) | 新记录固定写 `[]` |
| `status` | TINYINT | 0 可见，1 软删除 |
| `userId` | VARCHAR(255) | 历史字段，当前不使用 |
| `groupId` | BIGINT | 所属分组 id；`NULL` 表示未分组 |
| `groupName` | VARCHAR(255) | 冗余的分组名，查询与卡片配色直接用它 |
| `editTime` | DATETIME | 排序与保留策略依据；粘贴/去重时会刷新 |
| `shareType` | TINYINT | 历史字段，默认 0 |
| `isDelete` | TINYINT | 历史字段，默认 0 |
| `createTime` / `updateTime` | DATETIME | 插入/更新时写入 |

索引：`id` 唯一、`content`、`title`、`groupId`、`editTime`。

### `post_group`

| 列 | 说明 |
| --- | --- |
| `id` | 15 位随机数字 |
| `groupName` | 分组名 |
| `status` | 0 可见，1 已删除（查询只取 0） |
| `editTime` | 最近修改时间 |
| `userId` | 默认 0 |
| `isDelete` | 默认 0 |
| `sorted` | 升序排序值，默认 99999 |
| `createTime` / `updateTime` | 时间戳 |

“全部”不是数据库里的分组，而是界面固定在索引 0 的虚拟分组（`ALL_GROUP_ID`、
`ALL_GROUP_NAME`）；以 `全部` 作为 `groupName` 查询时不做分组过滤。

### `post_icon`

| 列 | 说明 |
| --- | --- |
| `id` | 主键 |
| `url` | 图标地址（兼容旧数据） |
| `hash` | 图标内容哈希 |
| `createTime` / `updateTime` | 时间戳 |

当前实现不写入这张表，而是把图标 PNG 直接缓存到 `icons/<hash>.png`；表保留用于
读取旧库。

## 3. `post.content` 的格式

| 类型 | `content` |
| --- | --- |
| 文本（0） | 剪贴板带 HTML 时存 HTML 片段（`StartFragment`/`EndFragment` 之间），否则存纯文本 |
| 图片（1） | `data:image/png;base64,...` |
| 链接（2） | URL 原文；识别规则：`http://`、`https://`、`ftp://` 开头，无空白，且首字符不是 `/ $ . ? #` |
| 文件（3）/ 文件夹（4） | JSON 文件描述符 |

文件描述符：

```json
{
  "content": "C:\\Users\\me\\Documents\\report.pdf",
  "url": "static://C:\\Users\\me\\Documents\\report.pdf",
  "size": 10240,
  "title": "report.pdf",
  "type": 1
}
```

`type`：0 图片文件、1 普通文件、2 文件夹。文件夹的 `size` 是递归求和的字节数。

## 4. 写入、去重与删除

- **插入**：写入 `createTime` / `updateTime`，`groupName` 初始为 `全部`，`groupId`
  为 `NULL`。
- **批量插入**：`INSERT OR IGNORE`，在事务内完成。
- **去重**：采集时用 `content + status = 0` 精确匹配；命中不新增记录，只把
  `editTime`（以及 `updateTime`）刷新为当前时间。
- **软删除**：单条删除、清空未分组项、清空某分组都只把 `status` 置 1 并刷新
  `editTime`，记录仍在库里。
- **硬删除**：设置里的“清空历史记录”执行 `DELETE FROM post`，同时清空
  `post_group` 和 `hotKeys`。
- **删除分组**：先 `clear_group_posts` 把组内记录软删除（`status = 1`），再把这些记录
  改成未分组（`groupId = NULL`、`groupName = 全部`，但仍是隐藏状态），最后软删除
  分组本身。实际效果是“删除分组并清空其内容”。
- **重命名分组**：先更新 `post_group.groupName`，再把该组所有记录的
  `groupName` 同步成新名字（面板路径会做这一步）。
- **保留策略**：按 `editTime < 截止时间` 直接 `DELETE`，见第 6 节。

## 5. 查询语义

`Db::query_posts(PostQuery)` 对应旧版 `query-post-data`：

| 条件 | 行为 |
| --- | --- |
| `id` | `id = ?` |
| `status` | `status = ?` |
| `group_name` | 非空且不是 `全部` 时按 `groupName = ?` 过滤 |
| `content` | 精确匹配，用于去重 |
| `title` | 非空时 `(title LIKE %kw% OR content LIKE %kw%)`，并排除图片（`contentType <> 1`） |
| 排序 | 始终 `ORDER BY editTime DESC` |
| 分页 | `limit` 默认 20（`PAGE_SIZE`），`current` 从 1 开始 |

面板实际请求最多 200 条，不做分页；旧接口的 20 条默认值只作为兜底。

## 6. 历史保留

`general.historyCache` 决定保留窗口：

| 值 | 含义 | 截止时间 |
| --- | --- | --- |
| `second` | 调试用 | 10 秒前 |
| `day` | 天 | 1 天前 |
| `week` | 周 | 7 天前 |
| `month` | 月 | 30 天前 |
| `quarter` | 季度 | 120 天前 |
| `half` | 半年 | 182 天前 |
| `year` | 年 | 365 天前 |
| `infinity` | 无限制（默认） | 不清理 |

触发时机：启动时、设置改动后立刻、以及事件泵连续空闲约 60 秒时。清理按 `editTime`
比较，因此被重新粘贴过的记录会重新计时。

## 7. 图标与临时文件

- 来源应用图标：`icon::icon_for_exe()` 用 `ExtractIconExW` + GDI 渲染 64×64 PNG，
  以 SHA-256 命名写入 `icons/`，同时把 `data:` URL 与哈希存进 `post.sourceApp`。
- 图片粘贴：`paste::write_temp_image()` 把 PNG 写到 `tmp/file_<毫秒>_<随机>.png`，
  10 秒后后台线程删除。写的是文件而不是位图，因此目标应用把它当图片文件粘贴。
- 远程音效：首次播放时下载到 `cache/`，之后直接复用。
