pub mod brand;
pub mod commands;
pub mod error;
pub mod ingest;
pub mod net;
pub mod rag;
pub mod security;
pub mod storage;

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

const QUICK_SEARCH_SHORTCUT: &str = "CmdOrCtrl+Shift+Space";

/// Shared by `run()` and the tests so both see the same handlers and setup.
fn builder<R: Runtime>(b: tauri::Builder<R>) -> tauri::Builder<R> {
    b.invoke_handler(tauri::generate_handler![
        commands::search,
        commands::open_source,
        commands::copy_citation,
        commands::hide_window,
        commands::list_folders,
        commands::get_settings,
        commands::set_air_gap,
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
        load_search(app.handle(), app.path().app_data_dir()?.join("models"));
        register_quick_search_shortcut(app.handle())?;
        Ok(())
    })
}

/// Loads the embedding model off the main thread. Until it is managed, or if it fails,
/// `search` returns `model_missing`.
fn load_search<R: Runtime>(app: &AppHandle<R>, models_dir: std::path::PathBuf) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match rag::embedder::BertEmbedder::load(&models_dir, rag::embedder::MODELS[0].id) {
            Ok(e) => {
                app.manage(rag::hybrid::SearchService::new(
                    Arc::new(e),
                    Box::new(rag::hybrid::NoopReranker),
                ));
            }
            // RagError messages carry no paths.
            Err(e) => tracing::warn!(error = %e, "embedding model not loaded; search is off"),
        }
    });
}

fn register_quick_search_shortcut<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _, event| {
                if event.state() == ShortcutState::Pressed {
                    toggle_quick_search(app);
                }
            })
            .build(),
    )?;
    // Another app may own the shortcut. Search still works from the main window, so do not fail startup.
    if let Err(e) = app.global_shortcut().register(QUICK_SEARCH_SHORTCUT) {
        tracing::warn!(error = %e, "quick-search shortcut not registered");
    }
    Ok(())
}

fn toggle_quick_search<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window("quick-search") else {
        return;
    };
    let res = if w.is_visible().unwrap_or(false) {
        w.hide()
    } else {
        w.show().and_then(|()| w.set_focus())
    };
    if let Err(e) = res {
        tracing::warn!(error = %e, "quick-search toggle failed");
    }
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
        invoke_with(w, cmd, tauri::ipc::InvokeBody::default())
    }

    fn invoke_with(
        w: &WebviewWindow<MockRuntime>,
        cmd: &str,
        body: tauri::ipc::InvokeBody,
    ) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                // Must be the app's local origin, or the request is matched as a remote URL.
                url: "tauri://localhost".parse().unwrap(),
                body,
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
        let query = || tauri::ipc::InvokeBody::Json(serde_json::json!({ "query": "x" }));
        let err = invoke_with(&quick, "search", query()).expect_err("no search service");
        assert_eq!(err["code"], "model_missing", "got {err}");
        assert!(invoke(&quick, "hide_window").is_ok());

        assert_not_implemented(invoke(&main, "list_folders"));
        assert_acl_denied(invoke_with(&main, "search", query()));
        assert_acl_denied(invoke(&quick, "set_air_gap"));
    }

    #[test]
    fn quick_search_searches_an_indexed_folder() {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (_db_dir, db) = storage::db::tests::temp_db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/plan.txt"), "The launch date moved to March.").unwrap();
        let report = ingest::pipeline::index_folder_once(
            &db,
            &ingest::parser::ParserRegistry::new(),
            &root,
            &ingest::chunker::ChunkerConfig::default(),
            &ingest::chunker::CharApprox,
            &rag::embedder::FakeEmbedder,
        )
        .unwrap();
        assert_eq!(report.new, 1);
        app.manage(db);
        app.manage(rag::hybrid::SearchService::new(
            Arc::new(rag::embedder::FakeEmbedder),
            Box::new(rag::hybrid::NoopReranker),
        ));
        let quick = window(&app, "quick-search");
        let body = tauri::ipc::InvokeBody::Json(serde_json::json!({ "query": "launch date" }));
        let res = invoke_with(&quick, "search", body)
            .unwrap()
            .deserialize::<serde_json::Value>()
            .unwrap();
        let hit = &res["hits"][0];
        assert_eq!(hit["relPath"], "sub/plan.txt");
        assert_eq!(hit["fileName"], "plan.txt");
        assert_eq!(hit["matchKind"], "both");
        assert!(res["topScore"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn set_air_gap_persists_and_get_settings_reads_it() {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (_dir, db) = storage::db::tests::temp_db();
        let guard = Arc::new(security::airgap::AirGapGuard::load(&db).unwrap());
        app.manage(net::Client::new(guard).unwrap());
        app.manage(db);
        let main = window(&app, "main");
        let net = app.state::<net::Client>();
        let db = app.state::<storage::db::Db>();
        assert!(net.guard().is_enabled());

        for on in [false, true] {
            let body = tauri::ipc::InvokeBody::Json(serde_json::json!({ "enabled": on }));
            assert!(invoke_with(&main, "set_air_gap", body).is_ok());
            assert_eq!(net.guard().is_enabled(), on);
            let reloaded = security::airgap::AirGapGuard::load(&db).unwrap();
            assert_eq!(reloaded.is_enabled(), on);
            let settings = invoke(&main, "get_settings").unwrap();
            assert_eq!(
                settings.deserialize::<serde_json::Value>().unwrap()["airGap"],
                on
            );
        }
    }
}
