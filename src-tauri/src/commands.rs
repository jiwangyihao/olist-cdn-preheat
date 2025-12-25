use crate::crawler::{crawl_site, crawl_sites_round_robin};
use crate::error::AppError;
use crate::error::AppResult;
use crate::models::{FileItem, RunSettings, SiteSettings};
use crate::preheater::{start_preheat, start_preheat_multi};
use crate::state::AppState;
use tauri::{AppHandle, State};
use uuid::Uuid;

#[tauri::command]
pub async fn crawl(app: AppHandle, settings: SiteSettings) -> AppResult<Vec<FileItem>> {
    crawl_site(&app, &settings).await
}

#[tauri::command]
pub async fn crawl_multi(app: AppHandle, settings_list: Vec<SiteSettings>) -> AppResult<Vec<FileItem>> {
    crawl_sites_round_robin(&app, &settings_list).await
}

#[tauri::command]
pub async fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    site_settings: SiteSettings,
    run_settings: RunSettings,
    files: Vec<FileItem>,
) -> AppResult<String> {
    let run_id = Uuid::new_v4().to_string();

    // Persist latest settings for future retry-after-finish scenarios.
    *state.site_settings.lock().unwrap() = Some(site_settings.clone());
    *state.site_settings_list.lock().unwrap() = None;
    *state.run_settings.lock().unwrap() = run_settings.clone();
    
    // Create cancellation channel
    let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
    *state.cancel_token.lock().unwrap() = Some(cancel_tx);

    // Create file injection channel
    let (file_tx, file_rx) = tokio::sync::mpsc::unbounded_channel();
    *state.run_sender.lock().unwrap() = Some(file_tx.clone());

    // Spawn the preheat task
    let app_handle = app.clone();
    let r_id = run_id.clone();
    
    tokio::spawn(async move {
        let _ = start_preheat(app_handle, r_id, files, site_settings, run_settings, cancel_rx, file_tx, file_rx).await;
    });

    Ok(run_id)
}

#[tauri::command]
pub async fn start_run_multi(
    app: AppHandle,
    state: State<'_, AppState>,
    site_settings_list: Vec<SiteSettings>,
    run_settings: RunSettings,
    files: Vec<FileItem>,
) -> AppResult<String> {
    let run_id = Uuid::new_v4().to_string();

    *state.site_settings.lock().unwrap() = None;
    *state.site_settings_list.lock().unwrap() = Some(site_settings_list.clone());
    *state.run_settings.lock().unwrap() = run_settings.clone();

    let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
    *state.cancel_token.lock().unwrap() = Some(cancel_tx);

    let (file_tx, file_rx) = tokio::sync::mpsc::unbounded_channel();
    *state.run_sender.lock().unwrap() = Some(file_tx.clone());

    let app_handle = app.clone();
    let r_id = run_id.clone();

    tokio::spawn(async move {
        let _ = start_preheat_multi(app_handle, r_id, files, site_settings_list, run_settings, cancel_rx, file_tx, file_rx).await;
    });

    Ok(run_id)
}

#[tauri::command]
pub async fn retry_files(
    app: AppHandle,
    state: State<'_, AppState>,
    files: Vec<FileItem>,
) -> AppResult<()> {
    let count = files.len();
    let sender = {
        let guard = state.run_sender.lock().unwrap();
        guard.clone()
    };

    if let Some(tx) = sender {
        println!("[retry_files] enqueueing files={}...", count);
        for file in files {
            tx.send(file).map_err(|_| {
                println!("[retry_files] send failed: receiver dropped");
                AppError::Api("重试任务发送失败：后台任务已结束或通道已关闭".to_string())
            })?;
        }

        println!("[retry_files] enqueue ok (files={})", count);
        return Ok(());
    }

    // No active run: auto-start a new run using last saved settings.
    let site_settings_list = { state.site_settings_list.lock().unwrap().clone() };
    let site_settings = { state.site_settings.lock().unwrap().clone() };

    let run_settings = { state.run_settings.lock().unwrap().clone() };

    let run_id = Uuid::new_v4().to_string();
    println!("[retry_files] auto-start new run (run_id={}, files={})", run_id, count);

    let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
    *state.cancel_token.lock().unwrap() = Some(cancel_tx);

    let (file_tx, file_rx) = tokio::sync::mpsc::unbounded_channel();
    *state.run_sender.lock().unwrap() = Some(file_tx.clone());

    let app_handle = app.clone();
    let r_id = run_id.clone();

    // Prefer multi-site restart if we have a saved list.
    if let Some(list) = site_settings_list {
        tokio::spawn(async move {
            let _ = start_preheat_multi(app_handle, r_id, files, list, run_settings, cancel_rx, file_tx, file_rx).await;
        });
        return Ok(());
    }

    let Some(site_settings) = site_settings else {
        println!("[retry_files] rejected: no saved site_settings (files={})", count);
        return Err(AppError::Api("当前没有运行中的任务，且未保存站点设置。请点击“开始”重新启动一次运行。".to_string()));
    };

    tokio::spawn(async move {
        let _ = start_preheat(app_handle, r_id, files, site_settings, run_settings, cancel_rx, file_tx, file_rx).await;
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_run(state: State<'_, AppState>) -> AppResult<()> {
    let tx = {
        let mut token = state.cancel_token.lock().unwrap();
        token.take()
    };

    if let Some(tx) = tx {
        let _ = tx.send(()).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn login(
    base_url: String,
    username: String,
    password: String,
    proxy_url: Option<String>,
) -> AppResult<String> {
    crate::api::login_with_password(
        &base_url,
        &username,
        &password,
        proxy_url.as_deref(),
    ).await
}
