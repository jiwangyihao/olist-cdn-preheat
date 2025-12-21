use crate::api::OpenListClient;
use crate::error::AppResult;
use crate::models::{FileItem, SiteSettings};
use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, Listener, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;
use std::time::Duration;
use tokio::time::sleep;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

pub async fn crawl_site(app: &AppHandle, settings: &SiteSettings) -> AppResult<Vec<FileItem>> {
    // Concurrent crawl controller:
    // - Start a new connection every 0.2s (independent of completion)
    // - Max 8 concurrent connections
    // - Pause starting new connections while any worker is solving WAF
    // - Queue grows dynamically as new dirs are discovered

    let base_settings = settings.clone();

    // Shared mutable auth state.
    let cookie_store: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(base_settings.cookie.clone()));
    let ua_store: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(base_settings.user_agent.clone()));
    let refresh_lock = Arc::new(AsyncMutex::new(()));
    let is_cancelled = Arc::new(AtomicBool::new(false));

    // WAF gate.
    let waf_active = Arc::new(AtomicUsize::new(0));

    // Results and stats.
    let files_out: Arc<AsyncMutex<Vec<FileItem>>> = Arc::new(AsyncMutex::new(Vec::new()));
    let scanned_dirs = Arc::new(AtomicUsize::new(0));
    let total_dirs = Arc::new(AtomicUsize::new(1));

    // Directory queue (grows during crawl).
    let mut pending_dirs = VecDeque::new();
    pending_dirs.push_back(base_settings.start_path.clone());
    let (dir_tx, mut dir_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Error propagation.
    let (err_tx, mut err_rx) = tokio::sync::mpsc::unbounded_channel::<crate::error::AppError>();

    // Concurrency.
    let max_concurrent = 8usize;
    let active = Arc::new(AtomicUsize::new(0));

    // Used for UI.
    let current_path: Arc<AsyncMutex<String>> = Arc::new(AsyncMutex::new(base_settings.start_path.clone()));
    let site_id_for_ui = base_settings.id.clone();

    let mut spawn_interval = tokio::time::interval(Duration::from_millis(200));
    let mut progress_interval = tokio::time::interval(Duration::from_millis(300));

    // Static settings cloned into workers.
    let base_url = base_settings.api_base_url.clone();
    let token = base_settings.token.clone();
    let proxy_url = base_settings.proxy_url.clone();
    let dir_password = base_settings.dir_password.clone();

    // Main dispatcher loop.
    loop {
        // Drain newly discovered dirs.
        while let Ok(dir) = dir_rx.try_recv() {
            pending_dirs.push_back(dir);
            total_dirs.fetch_add(1, Ordering::Relaxed);
        }

        // Any fatal error?
        if let Ok(err) = err_rx.try_recv() {
            is_cancelled.store(true, Ordering::Relaxed);
            return Err(err);
        }

        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }

        // Emit progress periodically.
        tokio::select! {
            _ = progress_interval.tick() => {
                let found_files = { files_out.lock().await.len() };
                let cp = { current_path.lock().await.clone() };
                let _ = app.emit("crawl:progress", serde_json::json!({
                    "scanned_dirs": scanned_dirs.load(Ordering::Relaxed),
                    "total_dirs": total_dirs.load(Ordering::Relaxed),
                    "found_files": found_files,
                    "current_path": cp,
                    "siteId": site_id_for_ui,
                }));
            }
            _ = spawn_interval.tick() => {
                // Spawn exactly one new connection per tick, up to max_concurrent.
                if waf_active.load(Ordering::Relaxed) > 0 {
                    continue;
                }
                if active.load(Ordering::Relaxed) >= max_concurrent {
                    continue;
                }

                let Some(dir) = pending_dirs.pop_front() else {
                    // No pending work: if all workers finished and no new dirs incoming, we are done.
                    if active.load(Ordering::Relaxed) == 0 && dir_rx.is_empty() {
                        break;
                    }
                    continue;
                };

                *current_path.lock().await = dir.clone();
                active.fetch_add(1, Ordering::Relaxed);

                let app = app.clone();
                let dir_tx = dir_tx.clone();
                let err_tx = err_tx.clone();
                let cookie_store = cookie_store.clone();
                let ua_store = ua_store.clone();
                let refresh_lock = refresh_lock.clone();
                let waf_active = waf_active.clone();
                let files_out = files_out.clone();
                let scanned_dirs = scanned_dirs.clone();
                let active_counter = active.clone();
                let is_cancelled = is_cancelled.clone();
                let base_url = base_url.clone();
                let token = token.clone();
                let proxy_url = proxy_url.clone();
                let dir_password = dir_password.clone();
                let site_id = base_settings.id.clone();

                tokio::spawn(async move {
                    struct ActiveGuard(Arc<AtomicUsize>);
                    impl Drop for ActiveGuard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    let _active_guard = ActiveGuard(active_counter);

                    if is_cancelled.load(Ordering::Relaxed) {
                        return;
                    }

                    // Per-dir scan (paged list).
                    let mut page: u32 = 1;
                    let per_page: u32 = 100;

                    loop {
                        // Retry with increasing backoff.
                        let mut attempts: u32 = 0;
                        let max_attempts: u32 = 6;

                        let (items, total) = loop {
                            attempts += 1;
                            if is_cancelled.load(Ordering::Relaxed) {
                                return;
                            }

                            let cookie = { cookie_store.read().await.clone() };
                            let ua = { ua_store.read().await.clone() };

                            let client = match OpenListClient::new(
                                base_url.clone(),
                                token.clone(),
                                proxy_url.clone(),
                                ua.clone(),
                                cookie.clone(),
                            ) {
                                Ok(c) => c,
                                Err(e) => {
                                    let _ = err_tx.send(e);
                                    return;
                                }
                            };

                            let result = client.list_files(
                                &dir,
                                dir_password.as_deref(),
                                page,
                                per_page
                            ).await;

                            match result {
                                Ok(res) => break res,
                                Err(e) => {
                                    let err_str = e.to_string();
                                    let is_waf = err_str.contains("Failed to parse JSON")
                                        && (err_str.contains("<script>") || err_str.contains("acw_sc__v2"));

                                    if is_waf {
                                        if attempts >= max_attempts {
                                            is_cancelled.store(true, Ordering::Relaxed);
                                            let _ = err_tx.send(e);
                                            return;
                                        }

                                        // Enter WAF state (pauses dispatcher spawn).
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

                                        // Solve challenge (dedup by lock).
                                        let current_cookie = cookie.clone();
                                        {
                                            let _guard = refresh_lock.lock().await;
                                            let fresh_cookie = cookie_store.read().await.clone();
                                            if fresh_cookie == current_cookie {
                                                match solve_waf_challenge(&app, &base_url, ua.as_deref()).await {
                                                    Ok((new_cookie, new_ua)) => {
                                                        *cookie_store.write().await = Some(new_cookie.clone());
                                                        *ua_store.write().await = Some(new_ua.clone());
                                                        let _ = app.emit("site:cookie_updated", serde_json::json!({
                                                            "siteId": site_id.clone(),
                                                            "cookie": new_cookie,
                                                            "userAgent": new_ua
                                                        }));
                                                    }
                                                    Err(err) => {
                                                        // Treat as a retryable failure with larger backoff.
                                                        if attempts >= max_attempts {
                                                            is_cancelled.store(true, Ordering::Relaxed);
                                                            let _ = err_tx.send(err);
                                                            return;
                                                        }
                                                        let backoff_ms = (1000u64 * (1u64 << (attempts.saturating_sub(1)).min(4)))
                                                            .min(15000);
                                                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                                        continue;
                                                    }
                                                }
                                            }
                                        }

                                        // After solving (or another worker solved), backoff increases with attempts.
                                        let backoff_ms = (1000u64 * (1u64 << (attempts.saturating_sub(1)).min(4)))
                                            .min(15000);
                                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                        continue;
                                    }

                                    if attempts < max_attempts {
                                        let backoff_ms = (500u64 * (1u64 << (attempts.saturating_sub(1)).min(4)))
                                            .min(10000);
                                        println!("Crawl request failed (Attempt {}): {}", attempts, e);
                                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                        continue;
                                    }

                                    is_cancelled.store(true, Ordering::Relaxed);
                                    let _ = err_tx.send(e);
                                    return;
                                }
                            }
                        };

                        if items.is_empty() {
                            break;
                        }

                        // Append results.
                        {
                            let mut out = files_out.lock().await;
                            for mut item in items {
                                // Tag each file with its site/group id so downstream preheat can route correctly.
                                item.site_id = site_id.clone();

                                if item.is_dir {
                                    let _ = dir_tx.send(item.path.clone());
                                } else {
                                    out.push(item);
                                }
                            }
                        }

                        if (page as u64 * per_page as u64) >= total {
                            break;
                        }
                        page += 1;
                    }

                    scanned_dirs.fetch_add(1, Ordering::Relaxed);
                });
            }
        }
    }

    let result = { files_out.lock().await.clone() };
    Ok(result)
}

pub async fn crawl_sites_round_robin(app: &AppHandle, settings_list: &[SiteSettings]) -> AppResult<Vec<FileItem>> {
    // Multi-site crawl controller (global fairness):
    // - Global max 8 concurrent connections
    // - Every 0.2s try to start ONE request, selecting site in round-robin order
    // - Each site keeps its own dir queue + auth/WAF state
    // - Progress event uses aggregated counters, plus (optional) current site id

    if settings_list.is_empty() {
        return Ok(Vec::new());
    }

    #[derive(Clone)]
    struct SiteCtx {
        settings: SiteSettings,
        cookie_store: Arc<RwLock<Option<String>>>,
        ua_store: Arc<RwLock<Option<String>>>,
        refresh_lock: Arc<AsyncMutex<()>>,
        waf_active: Arc<AtomicUsize>,
        pending_dirs: Arc<AsyncMutex<VecDeque<String>>>,
        scanned_dirs: Arc<AtomicUsize>,
        total_dirs: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        current_path: Arc<AsyncMutex<String>>,
    }

    let mut sites: Vec<SiteCtx> = Vec::with_capacity(settings_list.len());
    for s in settings_list.iter().cloned() {
        let mut q = VecDeque::new();
        q.push_back(s.start_path.clone());
        sites.push(SiteCtx {
            cookie_store: Arc::new(RwLock::new(s.cookie.clone())),
            ua_store: Arc::new(RwLock::new(s.user_agent.clone())),
            refresh_lock: Arc::new(AsyncMutex::new(())),
            waf_active: Arc::new(AtomicUsize::new(0)),
            pending_dirs: Arc::new(AsyncMutex::new(q)),
            scanned_dirs: Arc::new(AtomicUsize::new(0)),
            total_dirs: Arc::new(AtomicUsize::new(1)),
            active: Arc::new(AtomicUsize::new(0)),
            current_path: Arc::new(AsyncMutex::new(s.start_path.clone())),
            settings: s,
        });
    }

    // Results and global control
    let files_out: Arc<AsyncMutex<Vec<FileItem>>> = Arc::new(AsyncMutex::new(Vec::new()));
    let is_cancelled = Arc::new(AtomicBool::new(false));
    let active_total = Arc::new(AtomicUsize::new(0));

    let (dir_tx, mut dir_rx) = tokio::sync::mpsc::unbounded_channel::<(usize, String)>();
    let (err_tx, mut err_rx) = tokio::sync::mpsc::unbounded_channel::<crate::error::AppError>();

    let mut spawn_interval = tokio::time::interval(Duration::from_millis(200));
    let mut progress_interval = tokio::time::interval(Duration::from_millis(300));
    let mut rr_idx: usize = 0;
    let mut last_site_id: Option<String> = None;

    loop {
        // Drain newly discovered dirs
        while let Ok((site_i, dir)) = dir_rx.try_recv() {
            if let Some(site) = sites.get(site_i) {
                site.total_dirs.fetch_add(1, Ordering::Relaxed);
                site.pending_dirs.lock().await.push_back(dir);
            }
        }

        // Any fatal error?
        if let Ok(err) = err_rx.try_recv() {
            is_cancelled.store(true, Ordering::Relaxed);
            return Err(err);
        }

        if is_cancelled.load(Ordering::Relaxed) {
            break;
        }

        tokio::select! {
            _ = progress_interval.tick() => {
                let found_files = { files_out.lock().await.len() };
                let scanned_dirs_total: usize = sites.iter().map(|s| s.scanned_dirs.load(Ordering::Relaxed)).sum();
                let total_dirs_total: usize = sites.iter().map(|s| s.total_dirs.load(Ordering::Relaxed)).sum();

                // Best-effort current path (from last site we spawned)
                let mut current_path_val = String::new();
                if let Some(sid) = &last_site_id {
                    if let Some(site) = sites.iter().find(|s| s.settings.id == *sid) {
                        current_path_val = site.current_path.lock().await.clone();
                    }
                }

                let _ = app.emit("crawl:progress", serde_json::json!({
                    "scanned_dirs": scanned_dirs_total,
                    "total_dirs": total_dirs_total,
                    "found_files": found_files,
                    "current_path": current_path_val,
                    "siteId": last_site_id,
                }));
            }
            _ = spawn_interval.tick() => {
                // Try start ONE new request
                let max_concurrent = 8usize;
                if active_total.load(Ordering::Relaxed) >= max_concurrent {
                    continue;
                }

                // Find next site with pending work and not currently solving WAF.
                let n = sites.len();
                let mut picked: Option<usize> = None;
                for _ in 0..n {
                    let idx = rr_idx % n;
                    rr_idx = (rr_idx + 1) % n;
                    let site = &sites[idx];
                    if site.waf_active.load(Ordering::Relaxed) > 0 {
                        continue;
                    }
                    if site.active.load(Ordering::Relaxed) >= max_concurrent {
                        continue;
                    }
                    if site.pending_dirs.lock().await.is_empty() {
                        continue;
                    }
                    picked = Some(idx);
                    break;
                }

                let Some(site_i) = picked else {
                    // No spawnable site: check finish
                    let mut any_pending = false;
                    for s in sites.iter() {
                        if !s.pending_dirs.lock().await.is_empty() {
                            any_pending = true;
                            break;
                        }
                    }
                    if !any_pending && active_total.load(Ordering::Relaxed) == 0 && dir_rx.is_empty() {
                        break;
                    }
                    continue;
                };

                let site = sites[site_i].clone();
                let dir_opt = { site.pending_dirs.lock().await.pop_front() };
                let Some(dir) = dir_opt else { continue; };

                *site.current_path.lock().await = dir.clone();
                last_site_id = Some(site.settings.id.clone());

                site.active.fetch_add(1, Ordering::Relaxed);
                active_total.fetch_add(1, Ordering::Relaxed);

                let app = app.clone();
                let dir_tx = dir_tx.clone();
                let err_tx = err_tx.clone();
                let files_out = files_out.clone();
                let is_cancelled = is_cancelled.clone();
                let active_total = active_total.clone();

                tokio::spawn(async move {
                    struct ActiveGuard(Arc<AtomicUsize>, Arc<AtomicUsize>);
                    impl Drop for ActiveGuard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::Relaxed);
                            self.1.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    let _active_guard = ActiveGuard(site.active.clone(), active_total);

                    if is_cancelled.load(Ordering::Relaxed) {
                        return;
                    }

                    let base_url = site.settings.api_base_url.clone();
                    let token = site.settings.token.clone();
                    let proxy_url = site.settings.proxy_url.clone();
                    let dir_password = site.settings.dir_password.clone();
                    let site_id = site.settings.id.clone();

                    // Per-dir scan (paged list)
                    let mut page: u32 = 1;
                    let per_page: u32 = 100;

                    loop {
                        let mut attempts: u32 = 0;
                        let max_attempts: u32 = 6;

                        let (items, total) = loop {
                            attempts += 1;
                            if is_cancelled.load(Ordering::Relaxed) {
                                return;
                            }

                            let cookie = { site.cookie_store.read().await.clone() };
                            let ua = { site.ua_store.read().await.clone() };

                            let client = match OpenListClient::new(
                                base_url.clone(),
                                token.clone(),
                                proxy_url.clone(),
                                ua.clone(),
                                cookie.clone(),
                            ) {
                                Ok(c) => c,
                                Err(e) => {
                                    let _ = err_tx.send(e);
                                    return;
                                }
                            };

                            let result = client.list_files(&dir, dir_password.as_deref(), page, per_page).await;
                            match result {
                                Ok(res) => break res,
                                Err(e) => {
                                    let err_str = e.to_string();
                                    let is_waf = err_str.contains("Failed to parse JSON")
                                        && (err_str.contains("<script>") || err_str.contains("acw_sc__v2"));

                                    if is_waf {
                                        if attempts >= max_attempts {
                                            is_cancelled.store(true, Ordering::Relaxed);
                                            let _ = err_tx.send(e);
                                            return;
                                        }

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

                                        let current_cookie = cookie.clone();
                                        {
                                            let _guard = site.refresh_lock.lock().await;
                                            let fresh_cookie = site.cookie_store.read().await.clone();
                                            if fresh_cookie == current_cookie {
                                                match solve_waf_challenge(&app, &base_url, ua.as_deref()).await {
                                                    Ok((new_cookie, new_ua)) => {
                                                        *site.cookie_store.write().await = Some(new_cookie.clone());
                                                        *site.ua_store.write().await = Some(new_ua.clone());
                                                        let _ = app.emit("site:cookie_updated", serde_json::json!({
                                                            "siteId": site_id.clone(),
                                                            "cookie": new_cookie,
                                                            "userAgent": new_ua
                                                        }));
                                                    }
                                                    Err(err) => {
                                                        if attempts >= max_attempts {
                                                            is_cancelled.store(true, Ordering::Relaxed);
                                                            let _ = err_tx.send(err);
                                                            return;
                                                        }
                                                        let backoff_ms = (1000u64 * (1u64 << (attempts.saturating_sub(1)).min(4))).min(15000);
                                                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                                        continue;
                                                    }
                                                }
                                            }
                                        }

                                        let backoff_ms = (1000u64 * (1u64 << (attempts.saturating_sub(1)).min(4))).min(15000);
                                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                        continue;
                                    }

                                    if attempts < max_attempts {
                                        let backoff_ms = (500u64 * (1u64 << (attempts.saturating_sub(1)).min(4))).min(10000);
                                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                        continue;
                                    }

                                    is_cancelled.store(true, Ordering::Relaxed);
                                    let _ = err_tx.send(e);
                                    return;
                                }
                            }
                        };

                        if items.is_empty() {
                            break;
                        }

                        {
                            let mut out = files_out.lock().await;
                            for mut item in items {
                                item.site_id = site_id.clone();
                                if item.is_dir {
                                    let _ = dir_tx.send((site_i, item.path.clone()));
                                } else {
                                    out.push(item);
                                }
                            }
                        }

                        if (page as u64 * per_page as u64) >= total {
                            break;
                        }
                        page += 1;
                    }

                    site.scanned_dirs.fetch_add(1, Ordering::Relaxed);
                });
            }
        }
    }


    let out = files_out.lock().await.clone();
    Ok(out)
}

pub async fn solve_waf_challenge(app: &AppHandle, url: &str, user_agent: Option<&str>) -> AppResult<(String, String)> {
    let window_label = format!("waf_solver_{}", Uuid::new_v4().simple());
    
    // Default Project UA
    let default_ua = "olist-cdn-preheat/0.1 (Mozilla/5.0 compatible)";
    let ua_to_use = user_agent.unwrap_or(default_ua);
    
    // We must make the window visible because many browsers/WAF scripts 
    // stop executing JavaScript (requestAnimationFrame, etc.) when the window is hidden.
    // We make it small to minimize disruption.
    let mut builder = WebviewWindowBuilder::new(
        app,
        &window_label,
        WebviewUrl::External(url.parse().map_err(|e| crate::error::AppError::Api(format!("Invalid URL: {}", e)))?)
    )
    .title("正在进行安全验证...")
    .inner_size(400.0, 300.0)
    .visible(true)
    .incognito(true);

    // Set User-Agent
    builder = builder.user_agent(ua_to_use);

    let webview = builder.initialization_script(r#"
        (function() {
            console.log("WAF Solver started");
            
            function report(cookie, ua) {
                console.log("Reporting success:", cookie, ua);
                // Use Tauri event system
                if (window.__TAURI__ && window.__TAURI__.event) {
                    window.__TAURI__.event.emit('waf_result', {cookie: cookie, ua: ua});
                } else {
                    console.error("Tauri API not found!");
                }
            }

            var checkInterval = setInterval(function() {
                try {
                    var c = document.cookie;
                    if (c && c.includes('acw_sc__v2')) {
                        console.log("Cookie found:", c);
                        report(c, navigator.userAgent);
                        clearInterval(checkInterval);
                    }
                } catch(e) {
                    console.error(e);
                }
            }, 500);
            
            // Fallback: if page loaded but no specific cookie found after some time
            window.addEventListener('load', function() {
                setTimeout(function() {
                    console.log("Page loaded, reporting whatever we have");
                    report(document.cookie, navigator.userAgent);
                }, 2000);
            });
        })();
    "#)
    .build()
    .map_err(|e| crate::error::AppError::Api(format!("Failed to create webview: {}", e)))?;

    // Use a channel to receive the result from the event listener
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));
    
    // Listen for the event on the webview
    let tx_clone = tx.clone();
    let unlisten = webview.listen("waf_result", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            if let (Some(cookie), Some(ua)) = (payload["cookie"].as_str(), payload["ua"].as_str()) {
                if let Ok(mut tx_guard) = tx_clone.lock() {
                    if let Some(tx) = tx_guard.take() {
                        let _ = tx.send((cookie.to_string(), ua.to_string()));
                    }
                }
            }
        }
    });

    // Wait for result or timeout
    let result = tokio::select! {
        res = rx => {
            match res {
                Ok(val) => Ok(val),
                Err(_) => Err(crate::error::AppError::Api("WAF challenge failed: Channel closed".to_string())),
            }
        }
        _ = sleep(Duration::from_secs(30)) => {
            Err(crate::error::AppError::Api("WAF challenge failed: Timeout".to_string()))
        }
    };
    
    // Clean up
    webview.unlisten(unlisten);
    let _ = webview.close();
    
    result
}

