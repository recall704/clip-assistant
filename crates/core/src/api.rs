//! Client for the original service backend (`https://api.jianqiezhushou.com`).
//!
//! Only the endpoints the app still needs are kept: the update announcement,
//! telemetry, link metadata, uploads and asset downloads. The account,
//! membership and cloud-sync endpoints were removed together with the
//! corresponding features. Every call fails soft: the application stays fully
//! usable offline.

use std::path::PathBuf;

pub const BASE_URL: &str = "https://api.jianqiezhushou.com";

/// Envelope returned by every endpoint.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct Envelope<T = serde_json::Value> {
    #[serde(default)]
    pub code: i64,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

pub struct Api {
    pub base: String,
}

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

impl Api {
    pub fn new() -> Api {
        Api {
            base: BASE_URL.to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> anyhow::Result<Envelope<T>> {
        let resp = ureq::get(&self.url(path))
            .timeout(std::time::Duration::from_secs(5))
            .call()?;
        Ok(resp.into_json::<Envelope<T>>()?)
    }

    fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> anyhow::Result<Envelope<T>> {
        let resp = ureq::post(&self.url(path))
            .timeout(std::time::Duration::from_secs(5))
            .send_json(body)?;
        Ok(resp.into_json::<Envelope<T>>()?)
    }

    // ------------------------------------------------------------ reporting

    pub fn report_usage(&self, payload: &serde_json::Value) {
        let _: anyhow::Result<Envelope> = self.post_json("/api/report/usage", payload);
    }

    pub fn report_install(&self, payload: &serde_json::Value) {
        let _: anyhow::Result<Envelope> = self.post_json("/api/report/install", payload);
    }

    pub fn report_download(&self, payload: &serde_json::Value) {
        let _: anyhow::Result<Envelope> = self.post_json("/api/report/download", payload);
    }

    // --------------------------------------------------------------- posts

    /// `POST /api/post/link/info` — title + favicon for a URL.
    pub fn link_info(&self, url: &str) -> anyhow::Result<(String, String)> {
        #[derive(serde::Deserialize, Default)]
        struct D {
            #[serde(default)]
            title: String,
            #[serde(default)]
            icon: String,
        }
        let env: Envelope<D> = self.post_json("/api/post/link/info", &serde_json::json!({ "url": url }))?;
        let d = env.data.unwrap_or_default();
        Ok((d.title, d.icon))
    }

    // --------------------------------------------------------------- files

    pub fn upload_base64(&self, file_name: &str, biz: &str, base64: &str) -> anyhow::Result<String> {
        let env: Envelope<String> = self.post_json(
            "/api/file/upload/base64",
            &serde_json::json!({ "fileName": file_name, "biz": biz, "base64": base64 }),
        )?;
        Ok(env.data.unwrap_or_default())
    }

    // ------------------------------------------------------------- display

    pub fn display_all(&self) -> anyhow::Result<serde_json::Value> {
        let env: Envelope<serde_json::Value> = self.get_json("/api/display/all/get")?;
        Ok(env.data.unwrap_or(serde_json::json!({})))
    }

    pub fn display_get(&self, biz: &str) -> anyhow::Result<serde_json::Value> {
        let env: Envelope<serde_json::Value> =
            self.get_json(&format!("/api/display/get?biz={biz}"))?;
        Ok(env.data.unwrap_or(serde_json::json!({})))
    }
}

/// Stable device identifier: the first MAC address, as the original used.
pub fn device_id() -> String {
    mac_address::get_mac_address()
        .ok()
        .flatten()
        .map(|m| m.to_string())
        .unwrap_or_default()
}

/// Cache directory for downloaded assets (custom sounds, icons).
pub fn cache_dir() -> PathBuf {
    let dir = crate::data_dir().join("cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Download a URL to the local cache and return the file path.
pub fn download_to_cache(url: &str) -> Option<PathBuf> {
    let name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty() && s.len() < 128)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}.bin", crate::util::sha256_hex(url.as_bytes())));
    let path = cache_dir().join(&name);
    if path.exists() {
        return Some(path);
    }
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .ok()?;
    let mut reader = resp.into_reader();
    let mut buf = Vec::new();
    std::io::copy(&mut reader, &mut buf).ok()?;
    std::fs::write(&path, buf).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_is_available() {
        // Not all machines expose a MAC; the call must not panic either way.
        let _ = device_id();
    }

    #[test]
    fn url_join() {
        let api = Api::new();
        assert_eq!(api.url("/api/config/get"), format!("{BASE_URL}/api/config/get"));
    }

    #[test]
    fn envelope_defaults() {
        let env: Envelope = serde_json::from_str(r#"{"code":0}"#).unwrap();
        assert_eq!(env.code, 0);
        assert!(env.data.is_none());
    }
}
