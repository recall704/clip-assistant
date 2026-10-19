//! Domain model — mirrors the original `post`, `post_group` and `post_icon`
//! tables and the shared TypeScript `API` types one-for-one, including field
//! names, so the same SQLite files stay readable.

use serde::{Deserialize, Serialize};

/// Content kinds stored in `post.contentType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum ContentType {
    Text = 0,
    Image = 1,
    Link = 2,
    File = 3,
    Folder = 4,
}

impl ContentType {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => ContentType::Image,
            2 => ContentType::Link,
            3 => ContentType::File,
            4 => ContentType::Folder,
            _ => ContentType::Text,
        }
    }

    pub fn as_i64(self) -> i64 {
        self as i64
    }

    /// Default card title shown by the original UI (`constants/index.ts`).
    pub fn default_title(self) -> &'static str {
        match self {
            ContentType::Text => "文本",
            ContentType::Image => "图片",
            ContentType::Link => "链接",
            ContentType::File => "文件",
            ContentType::Folder => "文件夹",
        }
    }

    /// Chinese label used by the "指定内容快捷键" settings page.
    pub fn label(self) -> &'static str {
        self.default_title()
    }
}

impl Serialize for ContentType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for ContentType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(ContentType::from_i64(i64::deserialize(d)?))
    }
}

/// One clipboard entry. Field names follow the original `post` table/JS type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: i64,
    #[serde(default)]
    pub title: String,
    /// Raw payload: text, HTML, a `data:` PNG URL, or a JSON file descriptor.
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub content_type: i64,
    /// JSON: `{ "name", "icon", "platform", "hash" }`.
    #[serde(default)]
    pub source_app: Option<String>,
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<i64>,
    #[serde(default)]
    pub group_name: Option<String>,
    #[serde(default)]
    pub edit_time: String,
    #[serde(default)]
    pub share_type: i64,
    #[serde(default)]
    pub is_delete: i64,
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub update_time: Option<String>,
    #[serde(default)]
    pub sorted: Option<i64>,
}

impl Post {
    pub fn content_type(&self) -> ContentType {
        ContentType::from_i64(self.content_type)
    }
}

/// A named bucket that posts can be moved into.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostGroup {
    pub id: i64,
    pub group_name: String,
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub edit_time: String,
    #[serde(default)]
    pub user_id: i64,
    #[serde(default)]
    pub is_delete: i64,
    #[serde(default)]
    pub sorted: i64,
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub update_time: Option<String>,
}

/// A cached favicon/source-app icon.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostIcon {
    pub id: i64,
    pub url: String,
    pub hash: String,
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub update_time: Option<String>,
}

/// The synthetic "全部" (All) group the UI prepends to the group strip.
pub const ALL_GROUP_ID: i64 = 483_278_975_432;
pub const ALL_GROUP_NAME: &str = "全部";

/// File/folder descriptor stored inside `post.content` for FILE/FOLDER posts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDescriptor {
    /// Absolute path on disk.
    pub content: String,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub title: String,
    /// 0 = image file, 1 = regular file, 2 = folder.
    #[serde(rename = "type", default)]
    pub kind: i64,
    #[serde(default)]
    pub url: Option<String>,
}

/// Query parameters accepted by the post list (`API.PostQueryRequest`).
#[derive(Debug, Clone, Default)]
pub struct PostQuery {
    pub id: Option<i64>,
    pub title: Option<String>,
    pub group_name: Option<String>,
    pub content: Option<String>,
    pub status: Option<i64>,
    pub current: Option<i64>,
    pub limit: Option<i64>,
}

/// Page size used by the original list query.
pub const PAGE_SIZE: i64 = 20;
