//! SQLite storage (`~/.clipboard/db.sqlite`).
//!
//! Schema, column names and semantics mirror the Sequelize models of the
//! original app so an existing database keeps working unchanged.

use crate::model::{ContentType, Post, PostGroup, PostIcon, PostQuery, PAGE_SIZE};
use crate::util::{format_date_to_string, now_string};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

const POST_COLUMNS: &str = "id, title, content, description, contentType, sourceApp, tags, status, \
     userId, groupId, groupName, editTime, shareType, isDelete, createTime, updateTime";

impl Db {
    /// Open (creating if needed) the database at `path` and ensure the schema.
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.ensure_schema()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Db> {
        let db = Db {
            conn: Mutex::new(Connection::open_in_memory()?),
        };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS post (
                id          BIGINT PRIMARY KEY,
                title       VARCHAR(255) NOT NULL,
                content     TEXT NOT NULL,
                description TEXT,
                contentType TINYINT NOT NULL,
                sourceApp   VARCHAR(255),
                tags        VARCHAR(255),
                status      TINYINT NOT NULL DEFAULT 0,
                userId      VARCHAR(255),
                groupId     BIGINT,
                groupName   VARCHAR(255),
                editTime    DATETIME NOT NULL,
                shareType   TINYINT DEFAULT 0,
                isDelete    TINYINT NOT NULL DEFAULT 0,
                createTime  DATETIME,
                updateTime  DATETIME
            );
            CREATE UNIQUE INDEX IF NOT EXISTS post_id ON post (id);
            CREATE INDEX IF NOT EXISTS post_content ON post (content);
            CREATE INDEX IF NOT EXISTS post_title ON post (title);
            CREATE INDEX IF NOT EXISTS idx_post_group ON post (groupId);
            CREATE INDEX IF NOT EXISTS post_edit_time ON post (editTime);

            CREATE TABLE IF NOT EXISTS post_group (
                id         BIGINT PRIMARY KEY,
                groupName  VARCHAR(255) NOT NULL,
                status     TINYINT NOT NULL DEFAULT 0,
                editTime   DATETIME NOT NULL,
                userId     BIGINT DEFAULT 0,
                isDelete   TINYINT NOT NULL DEFAULT 0,
                sorted     INTEGER DEFAULT 99999,
                createTime DATETIME,
                updateTime DATETIME
            );
            CREATE UNIQUE INDEX IF NOT EXISTS post_group_id ON post_group (id);

            CREATE TABLE IF NOT EXISTS post_icon (
                id         BIGINT PRIMARY KEY,
                url        VARCHAR(255) NOT NULL,
                hash       VARCHAR(255) NOT NULL,
                createTime DATETIME,
                updateTime DATETIME
            );
            CREATE UNIQUE INDEX IF NOT EXISTS post_icon_id ON post_icon (id);
            "#,
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- posts

    pub fn insert_post(&self, post: &Post) -> Result<Post> {
        let conn = self.conn.lock().unwrap();
        let now = now_string();
        conn.execute(
            &format!(
                "INSERT INTO post ({POST_COLUMNS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
            ),
            params![
                post.id,
                post.title,
                post.content,
                post.description,
                post.content_type,
                post.source_app,
                post.tags,
                post.status,
                post.user_id,
                post.group_id,
                post.group_name,
                post.edit_time,
                post.share_type,
                post.is_delete,
                now,
                now,
            ],
        )?;
        let mut out = post.clone();
        out.create_time = Some(now.clone());
        out.update_time = Some(now);
        Ok(out)
    }

    pub fn bulk_insert_posts(&self, posts: &[Post]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let now = now_string();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(&format!(
                "INSERT OR IGNORE INTO post ({POST_COLUMNS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
            ))?;
            for p in posts {
                stmt.execute(params![
                    p.id,
                    p.title,
                    p.content,
                    p.description,
                    p.content_type,
                    p.source_app,
                    p.tags,
                    p.status,
                    p.user_id,
                    p.group_id,
                    p.group_name,
                    p.edit_time,
                    p.share_type,
                    p.is_delete,
                    now,
                    now,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_post(&self, id: i64, patch: &PostPatch) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut sets: Vec<String> = Vec::new();
        let mut vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(v) = &patch.title {
            sets.push("title = ?".into());
            vals.push(Box::new(v.clone()));
        }
        if let Some(v) = &patch.content {
            sets.push("content = ?".into());
            vals.push(Box::new(v.clone()));
        }
        if let Some(v) = &patch.group_id {
            sets.push("groupId = ?".into());
            vals.push(Box::new(*v));
        }
        if let Some(v) = &patch.group_name {
            sets.push("groupName = ?".into());
            vals.push(Box::new(v.clone()));
        }
        if let Some(v) = &patch.source_app {
            sets.push("sourceApp = ?".into());
            vals.push(Box::new(v.clone()));
        }
        if let Some(v) = &patch.edit_time {
            sets.push("editTime = ?".into());
            vals.push(Box::new(v.clone()));
        }
        if let Some(v) = &patch.share_type {
            sets.push("shareType = ?".into());
            vals.push(Box::new(*v));
        }
        if let Some(v) = &patch.status {
            sets.push("status = ?".into());
            vals.push(Box::new(*v));
        }
        if sets.is_empty() {
            return Ok(0);
        }
        sets.push("updateTime = ?".into());
        vals.push(Box::new(now_string()));
        let sql = format!("UPDATE post SET {} WHERE id = ?", sets.join(", "));
        vals.push(Box::new(id));
        let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
        Ok(conn.execute(&sql, refs.as_slice())?)
    }

    /// Soft delete: mark `status = 1` and refresh `editTime`.
    pub fn remove_post(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "UPDATE post SET status = 1, editTime = ? WHERE id = ?",
            params![format_date_to_string(chrono::Local::now().timestamp_millis()), id],
        )?)
    }

    /// Soft delete every post that has no group.
    pub fn remove_all_ungrouped(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "UPDATE post SET status = 1, editTime = ? WHERE groupId IS NULL",
            params![format_date_to_string(chrono::Local::now().timestamp_millis())],
        )?)
    }

    /// Soft delete everything inside one group.
    pub fn clear_group_posts(&self, group_id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "UPDATE post SET status = 1, editTime = ? WHERE groupId = ?",
            params![
                format_date_to_string(chrono::Local::now().timestamp_millis()),
                group_id
            ],
        )?)
    }

    /// Hard delete of the whole history.
    pub fn clear_all_posts(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM post", [])?)
    }

    /// Remove posts whose `editTime` is older than the given time string.
    pub fn delete_posts_before(&self, time: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM post WHERE editTime < ?", params![time])?)
    }

    /// `prePullData` — id + editTime for text/link posts only.
    pub fn all_post_ids(&self) -> Result<Vec<(i64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, editTime FROM post WHERE contentType IN (0, 2) ORDER BY editTime DESC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn posts_by_ids(&self, ids: &[i64]) -> Result<Vec<Post>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.conn.lock().unwrap();
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT {POST_COLUMNS} FROM post WHERE id IN ({placeholders}) ORDER BY editTime DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(refs.as_slice(), row_to_post)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The `query-post-data` handler.
    pub fn query_posts(&self, q: &PostQuery) -> Result<Vec<Post>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = format!("SELECT {POST_COLUMNS} FROM post WHERE 1=1");
        let mut vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(id) = q.id {
            sql.push_str(" AND id = ?");
            vals.push(Box::new(id));
        }
        if let Some(status) = q.status {
            sql.push_str(" AND status = ?");
            vals.push(Box::new(status));
        }
        if let Some(group) = q.group_name.as_deref() {
            if !group.is_empty() && group != crate::model::ALL_GROUP_NAME {
                sql.push_str(" AND groupName = ?");
                vals.push(Box::new(group.to_string()));
            }
        }
        if let Some(content) = q.content.as_deref() {
            sql.push_str(" AND content = ?");
            vals.push(Box::new(content.to_string()));
        }
        if let Some(kw) = q.title.as_deref().filter(|s| !s.is_empty()) {
            sql.push_str(" AND (title LIKE ? OR content LIKE ?) AND contentType <> ?");
            let like = format!("%{kw}%");
            vals.push(Box::new(like.clone()));
            vals.push(Box::new(like));
            vals.push(Box::new(ContentType::Image.as_i64()));
        }
        sql.push_str(" ORDER BY editTime DESC");
        let limit = q.limit.unwrap_or(PAGE_SIZE);
        let page = q.current.unwrap_or(1).max(1);
        sql.push_str(" LIMIT ? OFFSET ?");
        vals.push(Box::new(limit));
        vals.push(Box::new((page - 1) * limit));

        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), row_to_post)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --------------------------------------------------------------- groups

    pub fn insert_group(&self, g: &PostGroup) -> Result<PostGroup> {
        let conn = self.conn.lock().unwrap();
        let now = now_string();
        let edit = if g.edit_time.is_empty() { now.clone() } else { g.edit_time.clone() };
        conn.execute(
            "INSERT INTO post_group (id, groupName, status, editTime, userId, isDelete, sorted, createTime, updateTime) \
             VALUES (?,?,?,?,?,?,?,?,?)",
            params![g.id, g.group_name, g.status, edit, g.user_id, g.is_delete, g.sorted, now, now],
        )?;
        let mut out = g.clone();
        out.edit_time = edit;
        Ok(out)
    }

    pub fn query_groups(&self) -> Result<Vec<PostGroup>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, groupName, status, editTime, userId, isDelete, sorted, createTime, updateTime \
             FROM post_group WHERE status = 0 ORDER BY sorted ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PostGroup {
                id: r.get(0)?,
                group_name: r.get(1)?,
                status: r.get(2)?,
                edit_time: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                user_id: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                is_delete: r.get(5)?,
                sorted: r.get::<_, Option<i64>>(6)?.unwrap_or(99_999),
                create_time: r.get(7)?,
                update_time: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn update_group(&self, id: i64, name: Option<&str>, sorted: Option<i64>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut sets = vec!["editTime = ?".to_string()];
        let mut vals: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(format_date_to_string(chrono::Local::now().timestamp_millis()))];
        if let Some(n) = name {
            sets.push("groupName = ?".into());
            vals.push(Box::new(n.to_string()));
        }
        if let Some(s) = sorted {
            sets.push("sorted = ?".into());
            vals.push(Box::new(s));
        }
        vals.push(Box::new(id));
        let sql = format!("UPDATE post_group SET {} WHERE id = ?", sets.join(", "));
        let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
        Ok(conn.execute(&sql, refs.as_slice())?)
    }

    pub fn remove_group(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "UPDATE post_group SET status = 1, editTime = ? WHERE id = ?",
            params![format_date_to_string(chrono::Local::now().timestamp_millis()), id],
        )?)
    }

    pub fn all_group_ids(&self) -> Result<Vec<(i64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, editTime FROM post_group WHERE status = 0")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn groups_by_ids(&self, ids: &[i64]) -> Result<Vec<PostGroup>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.conn.lock().unwrap();
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT id, groupName, status, editTime, userId, isDelete, sorted, createTime, updateTime \
             FROM post_group WHERE id IN ({placeholders}) AND status = 0 ORDER BY sorted ASC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(refs.as_slice(), |r| {
            Ok(PostGroup {
                id: r.get(0)?,
                group_name: r.get(1)?,
                status: r.get(2)?,
                edit_time: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                user_id: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                is_delete: r.get(5)?,
                sorted: r.get::<_, Option<i64>>(6)?.unwrap_or(99_999),
                create_time: r.get(7)?,
                update_time: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn clear_all_groups(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM post_group", [])?)
    }

    // ---------------------------------------------------------------- icons

    pub fn insert_icon(&self, icon: &PostIcon) -> Result<PostIcon> {
        let conn = self.conn.lock().unwrap();
        let now = now_string();
        conn.execute(
            "INSERT OR IGNORE INTO post_icon (id, url, hash, createTime, updateTime) VALUES (?,?,?,?,?)",
            params![icon.id, icon.url, icon.hash, now, now],
        )?;
        Ok(icon.clone())
    }

    pub fn query_icon_by_hash(&self, hash: &str) -> Result<Option<PostIcon>> {
        let conn = self.conn.lock().unwrap();
        let r = conn
            .query_row(
                "SELECT id, url, hash, createTime, updateTime FROM post_icon WHERE hash = ? LIMIT 1",
                params![hash],
                |r| {
                    Ok(PostIcon {
                        id: r.get(0)?,
                        url: r.get(1)?,
                        hash: r.get(2)?,
                        create_time: r.get(3)?,
                        update_time: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(r)
    }
}

/// A partial update to a post (`update-post` IPC).
#[derive(Debug, Clone, Default)]
pub struct PostPatch {
    pub title: Option<String>,
    pub content: Option<String>,
    pub group_id: Option<Option<i64>>,
    pub group_name: Option<String>,
    pub source_app: Option<String>,
    pub edit_time: Option<String>,
    pub share_type: Option<i64>,
    pub status: Option<i64>,
}

fn row_to_post(r: &rusqlite::Row<'_>) -> rusqlite::Result<Post> {
    Ok(Post {
        id: r.get(0)?,
        title: r.get(1)?,
        content: r.get(2)?,
        description: r.get(3)?,
        content_type: r.get(4)?,
        source_app: r.get(5)?,
        tags: r.get(6)?,
        status: r.get(7)?,
        user_id: r.get(8)?,
        group_id: r.get(9)?,
        group_name: r.get(10)?,
        edit_time: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
        share_type: r.get::<_, Option<i64>>(12)?.unwrap_or(0),
        is_delete: r.get::<_, Option<i64>>(13)?.unwrap_or(0),
        create_time: r.get(14)?,
        update_time: r.get(15)?,
        sorted: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentType, Post};

    fn post(id: i64, content: &str, ct: ContentType, edit: &str) -> Post {
        Post {
            id,
            title: ct.default_title().into(),
            content: content.into(),
            content_type: ct.as_i64(),
            edit_time: edit.into(),
            status: 0,
            group_name: Some("全部".into()),
            ..Default::default()
        }
    }

    #[test]
    fn insert_query_order() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "a", ContentType::Text, "2024-01-01 10:00:00"))
            .unwrap();
        db.insert_post(&post(2, "b", ContentType::Text, "2024-01-02 10:00:00"))
            .unwrap();
        let rows = db.query_posts(&PostQuery { status: Some(0), ..Default::default() }).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, 2, "newest first");
    }

    #[test]
    fn search_skips_images() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "hello world", ContentType::Text, "2024-01-01 10:00:00"))
            .unwrap();
        db.insert_post(&post(2, "hello image", ContentType::Image, "2024-01-02 10:00:00"))
            .unwrap();
        let rows = db
            .query_posts(&PostQuery {
                title: Some("hello".into()),
                status: Some(0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 1);
    }

    #[test]
    fn dedup_by_content() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "same", ContentType::Text, "2024-01-01 10:00:00"))
            .unwrap();
        let found = db
            .query_posts(&PostQuery {
                content: Some("same".into()),
                status: Some(0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn soft_delete_and_clear() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "a", ContentType::Text, "2024-01-01 10:00:00"))
            .unwrap();
        db.insert_post(&post(2, "b", ContentType::Text, "2024-01-02 10:00:00"))
            .unwrap();
        db.remove_post(1).unwrap();
        let visible = db.query_posts(&PostQuery { status: Some(0), ..Default::default() }).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, 2);
        db.clear_all_posts().unwrap();
        assert_eq!(
            db.query_posts(&PostQuery { status: Some(0), ..Default::default() }).unwrap().len(),
            0
        );
    }

    #[test]
    fn groups_crud_and_ordering() {
        let db = Db::open_in_memory().unwrap();
        let g1 = PostGroup { id: 10, group_name: "工作".into(), sorted: 2, ..Default::default() };
        let g2 = PostGroup { id: 11, group_name: "生活".into(), sorted: 1, ..Default::default() };
        db.insert_group(&g1).unwrap();
        db.insert_group(&g2).unwrap();
        let list = db.query_groups().unwrap();
        assert_eq!(list[0].group_name, "生活", "sorted ascending");
        db.update_group(10, Some("工作2"), Some(0)).unwrap();
        let list = db.query_groups().unwrap();
        assert_eq!(list[0].group_name, "工作2");
        db.remove_group(10).unwrap();
        assert_eq!(db.query_groups().unwrap().len(), 1);
    }

    #[test]
    fn move_post_to_group() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "a", ContentType::Text, "2024-01-01 10:00:00"))
            .unwrap();
        db.update_post(
            1,
            &PostPatch {
                group_id: Some(Some(55)),
                group_name: Some("工作".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let rows = db.query_posts(&PostQuery { status: Some(0), ..Default::default() }).unwrap();
        assert_eq!(rows[0].group_id, Some(55));
        assert_eq!(rows[0].group_name.as_deref(), Some("工作"));
        let in_group = db
            .query_posts(&PostQuery {
                group_name: Some("工作".into()),
                status: Some(0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(in_group.len(), 1);
        let all = db
            .query_posts(&PostQuery {
                group_name: Some("全部".into()),
                status: Some(0),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.len(), 1, "'全部' means no group filter");
    }

    #[test]
    fn clear_group_only_marks_members() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "a", ContentType::Text, "2024-01-01 10:00:00"))
            .unwrap();
        db.insert_post(&post(2, "b", ContentType::Text, "2024-01-02 10:00:00"))
            .unwrap();
        db.update_post(1, &PostPatch { group_id: Some(Some(7)), ..Default::default() })
            .unwrap();
        db.clear_group_posts(7).unwrap();
        let visible = db.query_posts(&PostQuery { status: Some(0), ..Default::default() }).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, 2);
    }

    #[test]
    fn history_cache_deletes_old_rows() {
        let db = Db::open_in_memory().unwrap();
        db.insert_post(&post(1, "old", ContentType::Text, "2020-01-01 00:00:00"))
            .unwrap();
        db.insert_post(&post(2, "new", ContentType::Text, "2030-01-01 00:00:00"))
            .unwrap();
        let removed = db.delete_posts_before("2025-01-01 00:00:00").unwrap();
        assert_eq!(removed, 1);
    }
}
