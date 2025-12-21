use crate::error::AppResult;
use crate::models::{FileItem, FileStatus, FileUpdate, HeaderKV, RunSettings, SiteSettings, WarningItem};
use crate::crawler::solve_waf_challenge;
use crate::rate_limit::{TokenBucket, SpeedMonitor};
use futures::StreamExt;
use reqwest::Client;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Semaphore, RwLock, Mutex};

pub async fn start_preheat(
    app: AppHandle,
    run_id: String,
    files: Vec<FileItem>,
    site_settings: SiteSettings,
    run_settings: RunSettings,
    mut cancel_rx: tokio::sync::mpsc::Receiver<()>,
    file_tx: tokio::sync::mpsc::UnboundedSender<FileItem>,
    mut file_rx: tokio::sync::mpsc::UnboundedReceiver<FileItem>,
) -> AppResult<()> {
    // Base headers (User-Agent, Referer) - NO COOKIE HERE
    let mut headers = reqwest::header::HeaderMap::new();

    // Add Referer to mimic browser behavior
    if let Ok(val) = reqwest::header::HeaderValue::from_str(&site_settings.api_base_url) {
        headers.insert(reqwest::header::REFERER, val);
    }

    let mut builder = Client::builder()
        .redirect(if site_settings.follow_redirects.unwrap_or(true) {
            reqwest::redirect::Policy::default()
        } else {
            reqwest::redirect::Policy::none()
        })
        .default_headers(headers);

    if let Some(ua) = &site_settings.user_agent {
        if !ua.is_empty() {
            builder = builder.user_agent(ua);
        }
    }

    if let Some(proxy) = &site_settings.proxy_url {
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
    }

    let client = builder.build()?;

    // Adaptive Concurrency State
    let max_conn = run_settings.max_conn as usize;
    let min_conn = run_settings.min_conn.unwrap_or(5) as usize;
    let current_concurrency = Arc::new(AtomicUsize::new(min_conn));
    let semaphore = Arc::new(Semaphore::new(max_conn)); // Hard limit at max_conn
    
    // Rate Limiting & Speed Monitoring
    let rate_limit = run_settings.rate_limit_bytes_per_sec.unwrap_or(0);
    let token_bucket = if rate_limit > 0 {
        Some(Arc::new(Mutex::new(TokenBucket::new(rate_limit))))
    } else {
        None
    };
    let speed_monitor = Arc::new(Mutex::new(SpeedMonitor::new(5))); // 5s sliding window

    let site_settings = Arc::new(site_settings);
    let cookie_store = Arc::new(RwLock::new(site_settings.cookie.clone()));
    let refresh_lock = Arc::new(Mutex::new(()));
    let is_cancelled = Arc::new(AtomicBool::new(false));

    // WAF gate: pause spawning while any task is handling WAF, plus optional cooldown.
    let waf_active = Arc::new(AtomicUsize::new(0));
    let waf_cooldown_until: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    
    // Adaptive Controller Loop
    let controller_cancel = is_cancelled.clone();
    let controller_monitor = speed_monitor.clone();
    let controller_concurrency = current_concurrency.clone();
    let controller_app = app.clone();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if controller_cancel.load(Ordering::Relaxed) {
                break;
            }

            if rate_limit > 0 {
                let current_speed = {
                    let monitor = controller_monitor.lock().await;
                    monitor.current_speed()
                };
                
                let current_conn = controller_concurrency.load(Ordering::Relaxed);
                
                if current_speed < (rate_limit as f64 * 0.9) as u64 {
                    if current_conn < max_conn {
                        let new_conn = (current_conn + 1).min(max_conn);
                        controller_concurrency.store(new_conn, Ordering::Relaxed);
                    }
                } 
                
                let _ = controller_app.emit("run:speedUpdate", current_speed);
            }
        }
    });

    let active_tasks = Arc::new(AtomicUsize::new(0));
    let mut pending_files = std::collections::VecDeque::from(files);
    let mut tasks = Vec::new();
    let mut last_spawn_time = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or(Instant::now());
    let mut last_log_time = Instant::now();
    let mut idle_since: Option<Instant> = None;

    // Dispatcher Loop
    loop {
        let cooldown_active = {
            let guard = waf_cooldown_until.lock().await;
            guard.map(|t| t > Instant::now()).unwrap_or(false)
        };
        let waf_blocked = waf_active.load(Ordering::Relaxed) > 0 || cooldown_active;

        if last_log_time.elapsed() > Duration::from_secs(5) {
            println!(
                "Dispatcher Loop: Active={}, Pending={}, Limit={}, WAF Active={}, CooldownActive={}",
                active_tasks.load(Ordering::Relaxed),
                pending_files.len(),
                if rate_limit > 0 { current_concurrency.load(Ordering::Relaxed) } else { max_conn },
                waf_active.load(Ordering::Relaxed),
                cooldown_active
            );
            last_log_time = Instant::now();
        }

        // Check cancellation
        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }
        
        // Check cancel channel non-blocking
        if let Ok(_) = cancel_rx.try_recv() {
            is_cancelled.store(true, Ordering::Relaxed);
            break;
        }

        // 1. Check for new files from channel (non-blocking)
        while let Ok(file) = file_rx.try_recv() {
            println!("Received retry file: {}", file.path);
            pending_files.push_front(file);
            idle_since = None;
        }

        // 2. Idle finish: if no pending and no active for a short grace period, finish the run.
        let active = active_tasks.load(Ordering::Relaxed);
        if pending_files.is_empty() && active == 0 {
            if idle_since.is_none() {
                idle_since = Some(Instant::now());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            if idle_since.unwrap().elapsed() >= Duration::from_secs(2) && file_rx.is_empty() {
                break;
            }
            continue;
        } else {
            idle_since = None;
        }

        // 3. Spawn tasks if we have capacity
        let limit = if rate_limit > 0 {
            let current_speed = {
                let monitor = speed_monitor.lock().await;
                monitor.current_speed()
            };
            if current_speed < (rate_limit as f64 * 0.8) as u64 {
                max_conn
            } else {
                current_concurrency.load(Ordering::Relaxed)
            }
        } else {
            max_conn
        };
        
        while active_tasks.load(Ordering::Relaxed) < limit && !pending_files.is_empty() {
            if waf_blocked {
                break;
            }

            // Throttle: at most one new task per second
            if last_spawn_time.elapsed() < Duration::from_secs(1) {
                break;
            }

            if is_cancelled.load(Ordering::Relaxed) {
                break;
            }

            if let Some(file) = pending_files.pop_front() {
                // Acquire hard permit
                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        println!("Semaphore exhausted! Active={}, Limit={}", active_tasks.load(Ordering::Relaxed), limit);
                        pending_files.push_front(file);
                        break;
                    }
                };

                last_spawn_time = Instant::now();
                println!("Spawning task: {}", file.path);

                active_tasks.fetch_add(1, Ordering::Relaxed);

                let file = file.clone();
                let client = client.clone();
                let site_settings = site_settings.clone();
                let app = app.clone();
                let run_id = run_id.clone();
                let is_cancelled = is_cancelled.clone();
                let cookie_store = cookie_store.clone();
                let refresh_lock = refresh_lock.clone();
                let token_bucket = token_bucket.clone();
                let speed_monitor = speed_monitor.clone();
                let active_tasks = active_tasks.clone();
                let waf_active = waf_active.clone();
                let waf_cooldown_until = waf_cooldown_until.clone();
                let file_tx = file_tx.clone();

                let task = tokio::spawn(async move {
                    let _permit = permit; // Hold permit until task done
                    
                    struct ActiveGuard(Arc<AtomicUsize>);
                    impl Drop for ActiveGuard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    let _guard = ActiveGuard(active_tasks);

                    if is_cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    
                    let url_str = resolve_download_url(&file, &site_settings);
                    
                    let _ = app.emit("run:fileUpdate", FileUpdate {
                        run_id: run_id.clone(),
                        site_id: site_settings.id.clone(),
                        path: file.path.clone(),
                        status: FileStatus::Running,
                        attempt: Some(1),
                        url: Some(url_str.clone()),
                        final_url: None,
                        content_length: None,
                        response_headers: None,
                        bytes_read: 0,
                        http_status: None,
                        duration_ms: None,
                        warning: None,
                        error: None,
                    });

                    let start = Instant::now();
                    
                    let mut attempts = 0;
                    let max_attempts = 5; 
                    let mut cookie_refreshes = 0;
                    let max_cookie_refreshes = 3;

                    let final_result: AppResult<(u64, reqwest::StatusCode, Option<String>, Option<u64>, Vec<HeaderKV>)> = loop {
                        attempts += 1;
                        
                        let current_cookie = {
                            let guard = cookie_store.read().await;
                            guard.clone()
                        };

                        let result = download_file(
                            &client,
                            &url_str,
                            &app,
                            &run_id,
                            &site_settings.id,
                            &file.path,
                            &is_cancelled,
                            current_cookie.as_deref(),
                            token_bucket.clone(),
                            speed_monitor.clone(),
                        )
                        .await;
                        
                        match result {
                            Ok((bytes, status, f_url, content_length, resp_headers)) => {
                                if bytes >= 4300 && bytes <= 4350 {
                                    // WAF Detected
                                    struct WafGuard(Arc<AtomicUsize>);
                                    impl WafGuard {
                                        fn new(counter: Arc<AtomicUsize>) -> Self {
                                            counter.fetch_add(1, Ordering::Relaxed);
                                            Self(counter)
                                        }
                                    }
                                    impl Drop for WafGuard {
                                        fn drop(&mut self) {
                                            self.0.fetch_sub(1, Ordering::Relaxed);
                                        }
                                    }
                                    let _waf_guard = WafGuard::new(waf_active.clone());
                                    
                                    // Try internal refresh first
                                    if cookie_refreshes < max_cookie_refreshes {
                                        let _ = app.emit("run:fileUpdate", FileUpdate {
                                            run_id: run_id.clone(),
                                            site_id: site_settings.id.clone(),
                                            path: file.path.clone(),
                                            status: FileStatus::Running,
                                            attempt: Some(attempts),
                                            url: Some(url_str.clone()),
                                            final_url: f_url.clone(),
                                            content_length,
                                            response_headers: Some(resp_headers.clone()),
                                            bytes_read: bytes,
                                            http_status: Some(status.as_u16()),
                                            duration_ms: None,
                                            warning: None,
                                            error: Some("Auth failure, refreshing cookie...".to_string()),
                                        });

                                        {
                                            let _guard = refresh_lock.lock().await;
                                            let fresh_cookie = cookie_store.read().await.clone();
                                            if fresh_cookie != current_cookie {
                                                // Cookie already refreshed by another task
                                                // We can continue immediately
                                            } else {
                                                match solve_waf_challenge(&app, &site_settings.api_base_url, site_settings.user_agent.as_deref()).await {
                                                    Ok((new_cookie, new_ua)) => {
                                                        let mut writer = cookie_store.write().await;
                                                        *writer = Some(new_cookie.clone());

                                                        cookie_refreshes += 1;
                                                        if attempts > max_attempts - 3 {
                                                            attempts = max_attempts - 3;
                                                        }

                                                        // Clear cooldown if any.
                                                        {
                                                            let mut cd = waf_cooldown_until.lock().await;
                                                            *cd = None;
                                                        }

                                                        let _ = app.emit("site:cookie_updated", serde_json::json!({
                                                            "siteId": site_settings.id.clone(),
                                                            "cookie": new_cookie,
                                                            "userAgent": new_ua
                                                        }));
                                                    }
                                                    Err(_) => {
                                                        // Count failed attempts too, to avoid infinite loops.
                                                        cookie_refreshes += 1;
                                                    }
                                                }
                                            }
                                        }
                                        // Continue loop to retry download
                                        continue; 
                                    }

                                    // If we are here, internal retries exhausted.
                                    if file.retries < 3 {
                                        // Re-queue logic
                                        let mut new_file = file.clone();
                                        new_file.retries += 1;

                                        let _ = file_tx.send(new_file);
                                        // Pause new tasks for 5s (cooldown gate)
                                        {
                                            let mut cd = waf_cooldown_until.lock().await;
                                            *cd = Some(Instant::now() + Duration::from_secs(5));
                                        }
                                        
                                        // Emit status update so UI knows it's retrying
                                        let _ = app.emit("run:fileUpdate", FileUpdate {
                                            run_id: run_id.clone(),
                                            site_id: site_settings.id.clone(),
                                            path: file.path.clone(),
                                            status: FileStatus::Queued, // Back to queue
                                            attempt: Some(attempts),
                                            url: Some(url_str.clone()),
                                            final_url: f_url.clone(),
                                            content_length,
                                            response_headers: Some(resp_headers.clone()),
                                            bytes_read: bytes,
                                            http_status: Some(status.as_u16()),
                                            duration_ms: None,
                                            warning: None,
                                            error: Some(format!("WAF detected, re-queuing (Retry {}/3)...", file.retries + 1)),
                                        });
                                        
                                        // Exit this task
                                        return;
                                    } else {
                                        // Max retries reached
                                        break Ok((bytes, status, f_url, content_length, resp_headers));
                                    }
                                } else {
                                    let expected_size = file.size;
                                    if bytes < (expected_size as f64 * 0.95) as u64 {
                                        if attempts < max_attempts {
                                            tokio::time::sleep(Duration::from_secs(2)).await;
                                            continue;
                                        } else {
                                            break Err(crate::error::AppError::Api(format!("CDN缓存不完整 (预期 {}, 实际 {})，请联系网站维护者清除缓存", expected_size, bytes)));
                                        }
                                    }

                                    break Ok((bytes, status, f_url, content_length, resp_headers));
                                }
                            },
                            Err(e) => {
                                if attempts < max_attempts {
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                    continue;
                                }
                                break Err(e);
                            }
                        }
                    };

                    let duration = start.elapsed().as_millis() as u64;

                    match final_result {
                        Ok((bytes, status, final_url_opt, content_length, resp_headers)) => {
                            if bytes >= 4300 && bytes <= 4350 {
                                let _ = app.emit("run:fileUpdate", FileUpdate {
                                    run_id: run_id.clone(),
                                    site_id: site_settings.id.clone(),
                                    path: file.path.clone(),
                                    status: FileStatus::Failed,
                                    attempt: Some(attempts),
                                    url: Some(url_str.clone()),
                                    final_url: final_url_opt.clone(),
                                    content_length,
                                    response_headers: Some(resp_headers),
                                    bytes_read: bytes,
                                    http_status: Some(status.as_u16()),
                                    duration_ms: Some(duration),
                                    warning: None,
                                    error: Some("Auth failure (4.22KB)".to_string()),
                                });
                            } else {
                                let warning = if let Some(f_url) = &final_url_opt {
                                    if *f_url != url_str {
                                         Some(WarningItem {
                                            code: "REDIRECT".to_string(),
                                            message: "Redirect detected".to_string(),
                                            context: Some(serde_json::json!({
                                                "original": url_str,
                                                "final": f_url
                                            })),
                                        })
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                let _ = app.emit("run:fileUpdate", FileUpdate {
                                    run_id: run_id.clone(),
                                    site_id: site_settings.id.clone(),
                                    path: file.path.clone(),
                                    status: FileStatus::Done,
                                    attempt: Some(attempts),
                                    url: Some(url_str.clone()),
                                    final_url: final_url_opt.clone(),
                                    content_length,
                                    response_headers: Some(resp_headers),
                                    bytes_read: bytes,
                                    http_status: Some(status.as_u16()),
                                    duration_ms: Some(duration),
                                    warning,
                                    error: None,
                                });
                            }
                        }
                        Err(e) => {
                            let _ = app.emit("run:fileUpdate", FileUpdate {
                                run_id: run_id.clone(),
                                site_id: site_settings.id.clone(),
                                path: file.path.clone(),
                                status: FileStatus::Failed,
                                attempt: Some(attempts),
                                url: Some(url_str.clone()),
                                final_url: None,
                                content_length: None,
                                response_headers: None,
                                bytes_read: 0,
                                http_status: None,
                                duration_ms: Some(duration),
                                warning: None,
                                error: Some(e.to_string()),
                            });
                        }
                    }
                });
                
                tasks.push(task);
            }
        }

        // 4. Sleep briefly to avoid busy loop
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Wait for all spawned tasks to finish
    for task in tasks {
        let _ = task.await;
    }

    let _ = app.emit("run:finished", ());

    // Cleanup shared state so UI + retry behavior stays consistent.
    // (Avoid holding these locks for long in async context.)
    {
        let state = app.state::<crate::state::AppState>();
        *state.run_sender.lock().unwrap() = None;
        *state.cancel_token.lock().unwrap() = None;
    }

    Ok(())
}

pub async fn start_preheat_multi(
    app: AppHandle,
    run_id: String,
    files: Vec<FileItem>,
    site_settings_list: Vec<SiteSettings>,
    run_settings: RunSettings,
    mut cancel_rx: tokio::sync::mpsc::Receiver<()>,
    file_tx: tokio::sync::mpsc::UnboundedSender<FileItem>,
    mut file_rx: tokio::sync::mpsc::UnboundedReceiver<FileItem>,
) -> AppResult<()> {
    #[derive(Clone)]
    struct SiteRuntime {
        settings: Arc<SiteSettings>,
        client: Client,
        cookie_store: Arc<RwLock<Option<String>>>,
        refresh_lock: Arc<Mutex<()>>,
        waf_active: Arc<AtomicUsize>,
        waf_cooldown_until: Arc<Mutex<Option<Instant>>>,
    }

    if site_settings_list.is_empty() {
        return Ok(());
    }

    let site_order: Vec<String> = site_settings_list.iter().map(|s| s.id.clone()).collect();

    let mut sites: HashMap<String, SiteRuntime> = HashMap::new();
    for s in site_settings_list {
        // Build per-site client (UA/proxy/referer/redirect policy). Cookie handled per-request.
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&s.api_base_url) {
            headers.insert(reqwest::header::REFERER, val);
        }

        let mut builder = Client::builder()
            .redirect(if s.follow_redirects.unwrap_or(true) {
                reqwest::redirect::Policy::default()
            } else {
                reqwest::redirect::Policy::none()
            })
            .default_headers(headers);

        if let Some(ua) = &s.user_agent {
            if !ua.is_empty() {
                builder = builder.user_agent(ua);
            }
        }
        if let Some(proxy) = &s.proxy_url {
            if !proxy.is_empty() {
                builder = builder.proxy(reqwest::Proxy::all(proxy)?);
            }
        }

        let client = builder.build()?;
        let settings = Arc::new(s);
        let site_id = settings.id.clone();

        sites.insert(
            site_id,
            SiteRuntime {
                client,
                cookie_store: Arc::new(RwLock::new(settings.cookie.clone())),
                refresh_lock: Arc::new(Mutex::new(())),
                waf_active: Arc::new(AtomicUsize::new(0)),
                waf_cooldown_until: Arc::new(Mutex::new(None)),
                settings,
            },
        );
    }

    // Adaptive Concurrency State (global)
    let max_conn = run_settings.max_conn as usize;
    let min_conn = run_settings.min_conn.unwrap_or(5) as usize;
    let current_concurrency = Arc::new(AtomicUsize::new(min_conn));
    let semaphore = Arc::new(Semaphore::new(max_conn));

    // Rate limiting (global)
    let rate_limit = run_settings.rate_limit_bytes_per_sec.unwrap_or(0);
    let token_bucket = if rate_limit > 0 {
        Some(Arc::new(Mutex::new(TokenBucket::new(rate_limit))))
    } else {
        None
    };
    let speed_monitor = Arc::new(Mutex::new(SpeedMonitor::new(5)));

    let is_cancelled = Arc::new(AtomicBool::new(false));

    // Adaptive Controller Loop (global)
    {
        let controller_cancel = is_cancelled.clone();
        let controller_monitor = speed_monitor.clone();
        let controller_concurrency = current_concurrency.clone();
        let controller_app = app.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                if controller_cancel.load(Ordering::Relaxed) {
                    break;
                }

                if rate_limit > 0 {
                    let current_speed = {
                        let monitor = controller_monitor.lock().await;
                        monitor.current_speed()
                    };

                    let current_conn = controller_concurrency.load(Ordering::Relaxed);
                    if current_speed < (rate_limit as f64 * 0.9) as u64 {
                        if current_conn < max_conn {
                            let new_conn = (current_conn + 1).min(max_conn);
                            controller_concurrency.store(new_conn, Ordering::Relaxed);
                        }
                    }

                    let _ = controller_app.emit("run:speedUpdate", current_speed);
                }
            }
        });
    }

    // Build per-site queues
    let mut queues: HashMap<String, VecDeque<FileItem>> = HashMap::new();
    for id in site_order.iter() {
        queues.insert(id.clone(), VecDeque::new());
    }

    let default_site_id = if site_order.len() == 1 { Some(site_order[0].clone()) } else { None };
    for mut f in files {
        if f.site_id.is_empty() {
            if let Some(sid) = &default_site_id {
                f.site_id = sid.clone();
            }
        }
        if f.site_id.is_empty() {
            return Err(crate::error::AppError::Api("多站点运行需要每个文件都带有 siteId".to_string()));
        }
        if !sites.contains_key(&f.site_id) {
            return Err(crate::error::AppError::Api(format!("未知 siteId: {}", f.site_id)));
        }
        queues.entry(f.site_id.clone()).or_default().push_back(f);
    }

    let active_tasks = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    let mut last_spawn_time = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or(Instant::now());
    let mut last_log_time = Instant::now();
    let mut idle_since: Option<Instant> = None;

    // Dispatcher Loop
    loop {
        if last_log_time.elapsed() > Duration::from_secs(5) {
            let pending_total: usize = site_order.iter().map(|id| queues.get(id).map(|q| q.len()).unwrap_or(0)).sum();
            println!(
                "[multi] Dispatcher: Active={}, PendingTotal={}, Limit={}",
                active_tasks.load(Ordering::Relaxed),
                pending_total,
                if rate_limit > 0 { current_concurrency.load(Ordering::Relaxed) } else { max_conn },
            );
            last_log_time = Instant::now();
        }

        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }
        if let Ok(_) = cancel_rx.try_recv() {
            is_cancelled.store(true, Ordering::Relaxed);
            break;
        }

        // drain injected files
        while let Ok(mut f) = file_rx.try_recv() {
            if f.site_id.is_empty() {
                if let Some(sid) = &default_site_id {
                    f.site_id = sid.clone();
                }
            }
            if !f.site_id.is_empty() {
                queues.entry(f.site_id.clone()).or_default().push_front(f);
                idle_since = None;
            }
        }

        let active = active_tasks.load(Ordering::Relaxed);
        let pending_total: usize = site_order.iter().map(|id| queues.get(id).map(|q| q.len()).unwrap_or(0)).sum();

        // idle finish (grace)
        if pending_total == 0 && active == 0 {
            if idle_since.is_none() {
                idle_since = Some(Instant::now());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            if idle_since.unwrap().elapsed() >= Duration::from_secs(2) && file_rx.is_empty() {
                break;
            }
            continue;
        } else {
            idle_since = None;
        }

        let limit = if rate_limit > 0 {
            let current_speed = {
                let monitor = speed_monitor.lock().await;
                monitor.current_speed()
            };
            if current_speed < (rate_limit as f64 * 0.8) as u64 {
                max_conn
            } else {
                current_concurrency.load(Ordering::Relaxed)
            }
        } else {
            max_conn
        };

        while active_tasks.load(Ordering::Relaxed) < limit {
            if last_spawn_time.elapsed() < Duration::from_secs(1) {
                break;
            }
            if is_cancelled.load(Ordering::Relaxed) {
                break;
            }

            // pick the site with most remaining tasks (load balancing), skipping WAF-blocked sites.
            let mut picked_site: Option<String> = None;
            let mut best_len: usize = 0;
            for site_id in site_order.iter() {
                let q_len = queues.get(site_id).map(|q| q.len()).unwrap_or(0);
                if q_len == 0 {
                    continue;
                }
                let Some(site) = sites.get(site_id) else { continue; };

                let cooldown_active = {
                    let guard = site.waf_cooldown_until.lock().await;
                    guard.map(|t| t > Instant::now()).unwrap_or(false)
                };
                let waf_blocked = site.waf_active.load(Ordering::Relaxed) > 0 || cooldown_active;
                if waf_blocked {
                    continue;
                }

                if q_len > best_len {
                    best_len = q_len;
                    picked_site = Some(site_id.clone());
                }
            }

            let Some(site_id) = picked_site else {
                break;
            };

            let Some(file) = queues.get_mut(&site_id).and_then(|q| q.pop_front()) else {
                break;
            };

            let permit = match semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    queues.entry(site_id).or_default().push_front(file);
                    break;
                }
            };

            last_spawn_time = Instant::now();
            active_tasks.fetch_add(1, Ordering::Relaxed);

            let site = sites.get(&file.site_id).cloned();
            let Some(site) = site else {
                continue;
            };

            let file = file.clone();
            let app = app.clone();
            let run_id = run_id.clone();
            let is_cancelled = is_cancelled.clone();
            let token_bucket = token_bucket.clone();
            let speed_monitor = speed_monitor.clone();
            let active_tasks = active_tasks.clone();
            let file_tx = file_tx.clone();

            let task = tokio::spawn(async move {
                let _permit = permit;
                struct ActiveGuard(Arc<AtomicUsize>);
                impl Drop for ActiveGuard {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::Relaxed);
                    }
                }
                let _guard = ActiveGuard(active_tasks);

                if is_cancelled.load(Ordering::Relaxed) {
                    return;
                }

                let url_str = resolve_download_url(&file, &site.settings);
                let _ = app.emit("run:fileUpdate", FileUpdate {
                    run_id: run_id.clone(),
                    site_id: site.settings.id.clone(),
                    path: file.path.clone(),
                    status: FileStatus::Running,
                    attempt: Some(1),
                    url: Some(url_str.clone()),
                    final_url: None,
                    content_length: None,
                    response_headers: None,
                    bytes_read: 0,
                    http_status: None,
                    duration_ms: None,
                    warning: None,
                    error: None,
                });

                let start = Instant::now();
                let mut attempts = 0;
                let max_attempts = 5;
                let mut cookie_refreshes = 0;
                let max_cookie_refreshes = 3;

                let final_result: AppResult<(u64, reqwest::StatusCode, Option<String>, Option<u64>, Vec<HeaderKV>)> = loop {
                    attempts += 1;
                    let current_cookie = { site.cookie_store.read().await.clone() };

                    let result = download_file(
                        &site.client,
                        &url_str,
                        &app,
                        &run_id,
                        &site.settings.id,
                        &file.path,
                        &is_cancelled,
                        current_cookie.as_deref(),
                        token_bucket.clone(),
                        speed_monitor.clone(),
                    ).await;

                    match result {
                        Ok((bytes, status, f_url, content_length, resp_headers)) => {
                            if bytes >= 4300 && bytes <= 4350 {
                                // WAF detected
                                struct WafGuard(Arc<AtomicUsize>);
                                impl WafGuard {
                                    fn new(counter: Arc<AtomicUsize>) -> Self {
                                        counter.fetch_add(1, Ordering::Relaxed);
                                        Self(counter)
                                    }
                                }
                                impl Drop for WafGuard {
                                    fn drop(&mut self) {
                                        self.0.fetch_sub(1, Ordering::Relaxed);
                                    }
                                }
                                let _waf_guard = WafGuard::new(site.waf_active.clone());

                                if cookie_refreshes < max_cookie_refreshes {
                                    let _ = app.emit("run:fileUpdate", FileUpdate {
                                        run_id: run_id.clone(),
                                        site_id: site.settings.id.clone(),
                                        path: file.path.clone(),
                                        status: FileStatus::Running,
                                        attempt: Some(attempts),
                                        url: Some(url_str.clone()),
                                        final_url: f_url.clone(),
                                        content_length,
                                        response_headers: Some(resp_headers.clone()),
                                        bytes_read: bytes,
                                        http_status: Some(status.as_u16()),
                                        duration_ms: None,
                                        warning: None,
                                        error: Some("Auth failure, refreshing cookie...".to_string()),
                                    });

                                    {
                                        let _guard = site.refresh_lock.lock().await;
                                        let fresh_cookie = site.cookie_store.read().await.clone();
                                        if fresh_cookie == current_cookie {
                                            match solve_waf_challenge(&app, &site.settings.api_base_url, site.settings.user_agent.as_deref()).await {
                                                Ok((new_cookie, new_ua)) => {
                                                    *site.cookie_store.write().await = Some(new_cookie.clone());
                                                    cookie_refreshes += 1;
                                                    if attempts > max_attempts - 3 {
                                                        attempts = max_attempts - 3;
                                                    }
                                                    {
                                                        let mut cd = site.waf_cooldown_until.lock().await;
                                                        *cd = None;
                                                    }
                                                    let _ = app.emit("site:cookie_updated", serde_json::json!({
                                                        "siteId": site.settings.id.clone(),
                                                        "cookie": new_cookie,
                                                        "userAgent": new_ua
                                                    }));
                                                }
                                                Err(_) => {
                                                    cookie_refreshes += 1;
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                }

                                if file.retries < 3 {
                                    let mut new_file = file.clone();
                                    new_file.retries += 1;
                                    let _ = file_tx.send(new_file);
                                    {
                                        let mut cd = site.waf_cooldown_until.lock().await;
                                        *cd = Some(Instant::now() + Duration::from_secs(5));
                                    }
                                    let _ = app.emit("run:fileUpdate", FileUpdate {
                                        run_id: run_id.clone(),
                                        site_id: site.settings.id.clone(),
                                        path: file.path.clone(),
                                        status: FileStatus::Queued,
                                        attempt: Some(attempts),
                                        url: Some(url_str.clone()),
                                        final_url: f_url.clone(),
                                        content_length,
                                        response_headers: Some(resp_headers.clone()),
                                        bytes_read: bytes,
                                        http_status: Some(status.as_u16()),
                                        duration_ms: None,
                                        warning: None,
                                        error: Some(format!("WAF detected, re-queuing (Retry {}/3)...", file.retries + 1)),
                                    });
                                    return;
                                }

                                break Ok((bytes, status, f_url, content_length, resp_headers));
                            }

                            let expected_size = file.size;
                            if bytes < (expected_size as f64 * 0.95) as u64 {
                                if attempts < max_attempts {
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                    continue;
                                } else {
                                    break Err(crate::error::AppError::Api(format!(
                                        "CDN缓存不完整 (预期 {}, 实际 {})，请联系网站维护者清除缓存",
                                        expected_size, bytes
                                    )));
                                }
                            }

                            break Ok((bytes, status, f_url, content_length, resp_headers));
                        }
                        Err(e) => {
                            if attempts < max_attempts {
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                continue;
                            }
                            break Err(e);
                        }
                    }
                };

                let duration = start.elapsed().as_millis() as u64;

                match final_result {
                    Ok((bytes, status, final_url_opt, content_length, resp_headers)) => {
                        if bytes >= 4300 && bytes <= 4350 {
                            let _ = app.emit("run:fileUpdate", FileUpdate {
                                run_id: run_id.clone(),
                                site_id: site.settings.id.clone(),
                                path: file.path.clone(),
                                status: FileStatus::Failed,
                                attempt: Some(attempts),
                                url: Some(url_str.clone()),
                                final_url: final_url_opt.clone(),
                                content_length,
                                response_headers: Some(resp_headers),
                                bytes_read: bytes,
                                http_status: Some(status.as_u16()),
                                duration_ms: Some(duration),
                                warning: None,
                                error: Some("Auth failure (4.22KB)".to_string()),
                            });
                        } else {
                            let warning = if let Some(f_url) = &final_url_opt {
                                if *f_url != url_str {
                                    Some(WarningItem {
                                        code: "REDIRECT".to_string(),
                                        message: "Redirect detected".to_string(),
                                        context: Some(serde_json::json!({
                                            "original": url_str,
                                            "final": f_url
                                        })),
                                    })
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            let _ = app.emit("run:fileUpdate", FileUpdate {
                                run_id: run_id.clone(),
                                site_id: site.settings.id.clone(),
                                path: file.path.clone(),
                                status: FileStatus::Done,
                                attempt: Some(attempts),
                                url: Some(url_str.clone()),
                                final_url: final_url_opt.clone(),
                                content_length,
                                response_headers: Some(resp_headers),
                                bytes_read: bytes,
                                http_status: Some(status.as_u16()),
                                duration_ms: Some(duration),
                                warning,
                                error: None,
                            });
                        }
                    }
                    Err(e) => {
                        let _ = app.emit("run:fileUpdate", FileUpdate {
                            run_id: run_id.clone(),
                            site_id: site.settings.id.clone(),
                            path: file.path.clone(),
                            status: FileStatus::Failed,
                            attempt: Some(attempts),
                            url: Some(url_str.clone()),
                            final_url: None,
                            content_length: None,
                            response_headers: None,
                            bytes_read: 0,
                            http_status: None,
                            duration_ms: Some(duration),
                            warning: None,
                            error: Some(e.to_string()),
                        });
                    }
                }
            });

            tasks.push(task);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    for task in tasks {
        let _ = task.await;
    }

    let _ = app.emit("run:finished", ());
    {
        let state = app.state::<crate::state::AppState>();
        *state.run_sender.lock().unwrap() = None;
        *state.cancel_token.lock().unwrap() = None;
    }

    Ok(())
}




async fn download_file(
    client: &Client, 
    url: &str,
    app: &AppHandle,
    run_id: &str,
    site_id: &str,
    path: &str,
    is_cancelled: &AtomicBool,
    cookie: Option<&str>,
    token_bucket: Option<Arc<Mutex<TokenBucket>>>,
    speed_monitor: Arc<Mutex<SpeedMonitor>>
) -> AppResult<(u64, reqwest::StatusCode, Option<String>, Option<u64>, Vec<HeaderKV>)> {
    if is_cancelled.load(Ordering::Relaxed) {
        return Err(crate::error::AppError::Api("Cancelled".to_string()));
    }

    let mut req = client.get(url);
    if let Some(c) = cookie {
        req = req.header(reqwest::header::COOKIE, c);
    }

    let resp = req.send().await?;
    let status = resp.status();
    let final_url = resp.url().to_string();
    let content_length = resp.content_length();
    let resp_headers: Vec<HeaderKV> = resp
        .headers()
        .iter()
        .map(|(k, v)| HeaderKV {
            name: k.as_str().to_string(),
            value: v.to_str().unwrap_or("<non-utf8>").to_string(),
        })
        .collect();
    
    if !status.is_success() {
        return Err(crate::error::AppError::Api(format!("HTTP {}", status)));
    }

    let mut stream = resp.bytes_stream();
    let mut total_bytes = 0;
    let mut last_emit = Instant::now();

    while let Some(chunk) = stream.next().await {
        if is_cancelled.load(Ordering::Relaxed) {
            return Err(crate::error::AppError::Api("Cancelled".to_string()));
        }
        
        let chunk = chunk?;
        let len = chunk.len() as u64;
        
        // Rate Limiting
        if let Some(bucket) = &token_bucket {
            let mut acquired = false;
            while !acquired {
                if is_cancelled.load(Ordering::Relaxed) {
                    return Err(crate::error::AppError::Api("Cancelled".to_string()));
                }
                {
                    let mut b = bucket.lock().await;
                    if b.acquire(len) {
                        acquired = true;
                    }
                }
                if !acquired {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }

        // Speed Monitoring
        {
            let mut monitor = speed_monitor.lock().await;
            monitor.add_sample(len);
        }

        total_bytes += len;

        if last_emit.elapsed().as_millis() >= 200 { // Reduced emit frequency for smoother UI
            let _ = app.emit("run:fileUpdate", FileUpdate {
                run_id: run_id.to_string(),
                site_id: site_id.to_string(),
                path: path.to_string(),
                status: FileStatus::Running,
                attempt: Some(1),
                url: Some(url.to_string()),
                final_url: None,
                content_length: None,
                response_headers: None,
                bytes_read: total_bytes,
                http_status: Some(status.as_u16()),
                duration_ms: None,
                warning: None,
                error: None,
            });
            last_emit = Instant::now();
        }
    }

    if let Some(len) = content_length {
        if total_bytes != len {
            return Err(crate::error::AppError::Api(format!("Incomplete download: expected {} bytes, got {}", len, total_bytes)));
        }
    }

    Ok((total_bytes, status, Some(final_url), content_length, resp_headers))
}

fn resolve_download_url(file: &FileItem, settings: &SiteSettings) -> String {
    let base = settings.download_base_url.as_ref().unwrap_or(&settings.api_base_url);
    let base = base.trim_end_matches('/');
    
    let encoded_path = file.path.split('/')
        .map(|segment| urlencoding::encode(segment))
        .collect::<Vec<_>>()
        .join("/");
    
    format!("{}/d{}", base, encoded_path)
}
