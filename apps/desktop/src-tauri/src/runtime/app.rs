//! Tauri application bootstrap, tray lifecycle, and command registration.

use super::*;

#[cfg(target_os = "windows")]
pub(super) fn configured_webview2_browser_args() -> Option<String> {
    let port = std::env::var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV)
        .ok()?
        .trim()
        .parse::<u16>()
        .ok()?;
    if port == 0 {
        return None;
    }
    Some(format!(
        "{DEFAULT_WEBVIEW2_BROWSER_ARGS} --remote-debugging-port={port} --remote-debugging-address=127.0.0.1"
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn configure_webview2_browser_args(context: &mut tauri::Context<tauri::Wry>) {
    let Some(arguments) = configured_webview2_browser_args() else {
        return;
    };
    for window in &mut context.config_mut().app.windows {
        window.additional_browser_args = Some(arguments.clone());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    LOOM_EXITING.store(false, Ordering::Release);
    let mut context = tauri::generate_context!();
    #[cfg(target_os = "windows")]
    configure_webview2_browser_args(&mut context);

    let mut builder = tauri::Builder::default();
    let allow_multiple_instances = matches!(
        std::env::var("LOOM_SMOKE_ALLOW_MULTIPLE_INSTANCES").as_deref(),
        Ok("1")
    );
    if !allow_multiple_instances {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    }

    let run_result = builder
        .setup(|app| {
            let general =
                read_loom_persisted_general_settings().unwrap_or(LoomGeneralRuntimeSettings {
                    minimize_to_tray: true,
                });
            LOOM_CLOSE_TO_TRAY.store(general.minimize_to_tray, Ordering::Relaxed);
            let settings = read_loom_persisted_cache_settings().unwrap_or_default();
            std::thread::spawn(move || {
                let _ = apply_loom_cache_preferences(&settings);
            });
            std::thread::spawn(|| {
                if let Err(error) = start_loom_daemon_blocking() {
                    eprintln!("[ERROR] Loom 本地服务启动失败：{error}");
                }
            });
            if let Some(window) = app.get_webview_window("main") {
                let icon = tauri::image::Image::from_bytes(include_bytes!("../../icons/icon.png"))?;
                window.set_icon(icon)?;
            }
            let show_item = MenuItem::with_id(app, "show", "显示 Loom", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray_builder = TrayIconBuilder::with_id("loom")
                .menu(&tray_menu)
                .tooltip("Loom")
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        begin_desktop_exit();
                        app.exit(0);
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            app.manage(tray_builder.build(app)?);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if LOOM_CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    begin_desktop_exit();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            daemon::resolve_loom_daemon_url,
            diagnostics::resolve_application_diagnostics,
            diagnostics::open_application_log_location,
            diagnostics::open_external_url,
            diagnostics::open_mcp_source_url,
            loom_cache::apply_loom_general_settings,
            loom_cache::get_loom_cache_snapshot,
            loom_cache::apply_loom_cache_settings,
            loom_cache::clear_loom_cache,
            hook_cache::get_hook_cache_snapshot,
            hook_cache::wait_for_hook_cache_settings,
            hook_cache::clear_hook_cache,
            daemon::read_loom_snapshot,
            commands::start_loom_daemon,
            commands::get_loom_daemon_json,
            commands::put_loom_daemon_json,
            commands::delete_loom_daemon_json,
            commands::post_loom_daemon_json,
            commands::install_packaged_framework,
            commands::bootstrap_packaged_arts,
            commands::read_hook_canvas_preview
        ])
        .run(context);
    begin_desktop_exit();
    run_result.expect("error while running Loom desktop");
}
