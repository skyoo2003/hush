pub mod brand;
pub mod commands;
pub mod entitlement;
pub mod error;
pub mod ingest;
pub mod net;
pub mod rag;
pub mod security;
pub mod storage;

use std::sync::Arc;

use tauri::{Manager, Runtime};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

/// Shared by `run()` and the tests so both see the same handlers and setup.
fn builder<R: Runtime>(b: tauri::Builder<R>) -> tauri::Builder<R> {
    b.invoke_handler(tauri::generate_handler![
        commands::search,
        commands::open_source,
        commands::copy_citation,
        commands::hide_window,
        commands::list_folders,
        commands::get_settings,
        commands::get_license_status,
        commands::list_models,
        commands::export_pack,
    ])
    .setup(|app| {
        init_tracing(&app.path().app_log_dir()?)?;
        let labels: Vec<String> = app.webview_windows().into_keys().collect();
        tracing::info!(?labels, "app started");
        let db = open_storage(&app.path().app_data_dir()?)
            .inspect_err(|e| tracing::error!(error = %e, "storage open failed"))?;
        let guard = Arc::new(security::airgap::AirGapGuard::load(&db)?);
        app.manage(net::Client::new(guard)?);
        app.manage(db);
        Ok(())
    })
}

fn open_storage(dir: &std::path::Path) -> Result<storage::db::Db, storage::error::StorageError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.db", brand::APP_SLUG));
    storage::db::open_or_create(&path, &storage::keychain::KeychainKeyProvider)
}

fn init_tracing(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    // Create first: the appender prunes old files at build and prints an error if the dir is missing.
    std::fs::create_dir_all(dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(brand::APP_SLUG)
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // try_init: a subscriber may already be set (tests build the app more than once).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(appender)
        .try_init();
    Ok(())
}

pub fn run() {
    if builder(tauri::Builder::default())
        .run(tauri::generate_context!())
        .is_err()
    {
        // No error details: they can carry paths.
        eprintln!("startup failed");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

    fn window(app: &tauri::App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
        app.get_webview_window(label).unwrap_or_else(|| {
            WebviewWindowBuilder::new(app, label, WebviewUrl::default())
                .build()
                .unwrap()
        })
    }

    fn invoke(
        w: &WebviewWindow<MockRuntime>,
        cmd: &str,
    ) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                // Must be the app's local origin, or the request is matched as a remote URL.
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    fn assert_acl_denied(res: Result<tauri::ipc::InvokeResponseBody, serde_json::Value>) {
        let err = res.expect_err("expected ACL rejection");
        let msg = err.as_str().expect("ACL rejection is a plain string");
        assert!(msg.contains("not allowed"), "unexpected rejection: {msg}");
    }

    fn assert_not_implemented(res: Result<tauri::ipc::InvokeResponseBody, serde_json::Value>) {
        let err = res.expect_err("expected AppError");
        assert_eq!(err["code"], "not_implemented", "got {err}");
    }

    #[test]
    fn acl_separates_windows() {
        // Real config + capabilities; `test = true` only skips the macOS Info.plist symbol,
        // which would otherwise be defined twice next to run()'s generate_context!().
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let main = window(&app, "main");
        let quick = window(&app, "quick-search");

        assert_acl_denied(invoke(&quick, "get_settings"));
        assert_not_implemented(invoke(&quick, "search"));
        assert!(invoke(&quick, "hide_window").is_ok());

        assert_not_implemented(invoke(&main, "get_settings"));
        assert_acl_denied(invoke(&main, "search"));
    }
}
