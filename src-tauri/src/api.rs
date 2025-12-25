use crate::error::{AppError, AppResult};
use crate::models::FileItem;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use chrono::{DateTime, Utc};

// ============ Login API Structures ============

#[derive(Debug, Serialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    code: i32,
    message: String,
    data: Option<LoginData>,
}

#[derive(Debug, Deserialize)]
struct LoginData {
    token: String,
}

// ============ File List API Structures ============

#[derive(Debug, Deserialize)]
struct FsListResponse {
    code: i32,
    message: String,
    data: Option<FsListData>,
}

#[derive(Debug, Deserialize)]
struct FsListData {
    #[serde(default)]
    content: Option<Vec<FsObject>>,
    #[serde(default)]
    total: u64,
}

#[derive(Debug, Deserialize)]
struct FsObject {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    is_dir: bool,
    modified: Option<String>,
    // created: String,
    // sign: String,
    // thumb: String,
    // type: i32,
    // path: String, // System path, ignore
}

pub struct OpenListClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl OpenListClient {
    pub fn new(
        base_url: String,
        token: Option<String>,
        proxy_url: Option<String>,
        user_agent: Option<String>,
        cookie: Option<String>,
    ) -> AppResult<Self> {
        let mut builder = Client::builder()
            .timeout(std::time::Duration::from_secs(30));

        if let Some(ua) = user_agent {
            if !ua.is_empty() {
                builder = builder.user_agent(ua);
            }
        }

        if let Some(c) = cookie {
            if !c.is_empty() {
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::COOKIE,
                    reqwest::header::HeaderValue::from_str(&c).map_err(|e| AppError::Api(format!("Invalid cookie: {}", e)))?,
                );
                builder = builder.default_headers(headers);
            }
        }

        if let Some(proxy) = proxy_url {
            if !proxy.is_empty() {
                builder = builder.proxy(reqwest::Proxy::all(&proxy)?);
            }
        }

        let client = builder.build()?;
        
        // Normalize base url (remove trailing slash)
        let base_url = base_url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            token,
        })
    }

    pub async fn list_files(&self, path: &str, password: Option<&str>, page: u32, per_page: u32) -> AppResult<(Vec<FileItem>, u64)> {
        let url = format!("{}/api/fs/list", self.base_url);
        
        let mut body = json!({
            "path": path,
            "page": page,
            "per_page": per_page,
            "refresh": false
        });

        if let Some(pwd) = password {
            body["password"] = json!(pwd);
        }

        let mut req = self.client.post(&url).json(&body);
        
        if let Some(token) = &self.token {
            if !token.is_empty() {
                req = req.header("Authorization", token);
            }
        }

        let resp = req.send().await?;
        let status = resp.status();
        
        if !status.is_success() {
             return Err(AppError::Api(format!("HTTP Error: {}", status)));
        }

        let text = resp.text().await?;
        let json_resp: FsListResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Api(format!("Failed to parse JSON: {}. Response: {}", e, text))
        })?;

        if json_resp.code != 200 {
            return Err(AppError::Api(format!("API Error {}: {}", json_resp.code, json_resp.message)));
        }

        let data = json_resp.data.ok_or_else(|| AppError::Api("No data in response".to_string()))?;

        let items = data.content.unwrap_or_default().into_iter().map(|obj| {
            // Parse modified time if possible, else None
            let modified = obj.modified.as_ref().and_then(|m| {
                DateTime::parse_from_rfc3339(m)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            });

            // Construct logical path: parent_path + / + name
            // Ensure path starts with / and ends with / if it's a dir (optional, but good for logic)
            // Actually, let's keep it clean.
            let clean_parent = if path == "/" { "" } else { path.trim_end_matches('/') };
            let logical_path = format!("{}/{}", clean_parent, obj.name);

            FileItem {
                id: uuid::Uuid::new_v4().to_string(), // Generate a temp ID
                site_id: String::new(),
                path: logical_path,
                name: obj.name,
                size: obj.size,
                is_dir: obj.is_dir,
                modified,
                retries: 0,
            }
        }).collect();

        Ok((items, data.total))
    }
}

// ============ Standalone Login Function ============

/// Login to OpenList with username and password, returns JWT token.
/// This is a standalone function that doesn't require an OpenListClient instance.
pub async fn login_with_password(
    base_url: &str,
    username: &str,
    password: &str,
    proxy_url: Option<&str>,
) -> AppResult<String> {
    let mut builder = Client::builder()
        .timeout(std::time::Duration::from_secs(30));

    if let Some(proxy) = proxy_url {
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
    }

    let client = builder.build()?;
    let base_url = base_url.trim_end_matches('/');
    let url = format!("{}/api/auth/login", base_url);

    let body = LoginRequest {
        username: username.to_string(),
        password: password.to_string(),
    };

    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();

    if !status.is_success() {
        return Err(AppError::Api(format!("HTTP Error: {}", status)));
    }

    let text = resp.text().await?;
    let json_resp: LoginResponse = serde_json::from_str(&text).map_err(|e| {
        AppError::Api(format!("Failed to parse login response: {}. Response: {}", e, text))
    })?;

    if json_resp.code != 200 {
        return Err(AppError::Api(format!("Login failed: {}", json_resp.message)));
    }

    let data = json_resp.data.ok_or_else(|| AppError::Api("No token in login response".to_string()))?;
    
    Ok(data.token)
}
