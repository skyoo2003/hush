// SPDX-License-Identifier: GPL-3.0-or-later
pub mod about;
pub mod appearance;
pub mod brand;
pub mod commands;
pub mod desktop;
pub mod diagnostics;
pub mod error;
pub mod ingest;
pub mod net;
pub mod rag;
pub mod security;
pub mod sponsor;
pub mod storage;
pub mod update;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Listener, Manager, Runtime};
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
        commands::add_watched_folder,
        commands::remove_watched_folder,
        commands::set_folder_enabled,
        commands::list_failed_documents,
        commands::retry_document,
        commands::set_exclude_patterns,
        commands::get_index_status,
        commands::get_schedule,
        commands::set_schedule,
        commands::get_settings,
        commands::set_air_gap,
        commands::set_shortcut,
        commands::set_supporter,
        commands::set_theme,
        commands::set_accent,
        commands::get_appearance,
        commands::dismiss_sponsor_banner,
        commands::open_sponsor,
        commands::list_models,
        commands::download_model,
        commands::cancel_model_download,
        commands::import_model_file,
        commands::export_pack,
        commands::check_for_update,
        commands::install_update,
        commands::export_diagnostics,
        commands::delete_all_indexes,
        commands::get_about,
        commands::read_legal_text,
    ])
    // No window is granted a dialog permission: only `add_watched_folder` opens the picker.
    .plugin(tauri_plugin_dialog::init())
    // No window is granted an updater permission either: `update.rs` fetches through `net::Client`.
    .plugin(tauri_plugin_updater::Builder::new().build())
    .manage(update::Pending::default())
    .on_window_event(desktop::on_window_event)
    .setup(|app| {
        let log_dir = app.path().app_log_dir()?;
        init_tracing(&log_dir)?;
        diagnostics::install_panic_hook(log_dir.join(diagnostics::CRASH_DIR));
        let labels: Vec<String> = app.webview_windows().into_keys().collect();
        tracing::info!(?labels, "app started");
        let db = open_storage(&app.path().app_data_dir()?)
            .inspect_err(|e| tracing::error!(error = %e, "storage open failed"))?;
        let db = Arc::new(db);
        let guard = Arc::new(security::airgap::AirGapGuard::load(&db)?);
        let air_gap = guard.is_enabled();
        app.manage(net::Client::new(guard)?);
        app.manage(db.clone());
        desktop::setup(app.handle(), &db, air_gap)?;
        appearance::watch_accent(app.handle().clone());
        app.manage(rag::models::ModelManager::new(
            app.path().app_data_dir()?.join("models"),
        ));
        load_model(app.handle());
        tracing::info!("app ready");
        Ok(())
    })
}

/// Loads the first installed embedding model off the main thread, then starts search and
/// indexing. Until it is managed, or if it fails, `search` returns `model_missing` and nothing is
/// indexed. Called at startup and after a model install; only the first successful call loads.
pub(crate) fn load_model<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (Some(models), Some(db)) = (
            app.try_state::<rag::models::ModelManager>(),
            app.try_state::<Arc<storage::db::Db>>(),
        ) else {
            return;
        };
        let installed = rag::embedder::MODELS
            .iter()
            .find(|s| models.is_installed(s.id));
        let Some(spec) = installed else {
            tracing::info!("no embedding model installed; search is off");
            return;
        };
        if !models.claim_embedder_load() {
            return;
        }
        match rag::embedder::BertEmbedder::load(models.dir(), spec.id) {
            Ok(e) => {
                let e = Arc::new(e);
                app.manage(rag::hybrid::SearchService::new(
                    e.clone(),
                    Box::new(rag::hybrid::NoopReranker),
                ));
                start_indexing(&app, db.inner().clone(), e);
            }
            Err(e) => {
                models.release_embedder_load();
                // RagError messages carry no paths.
                tracing::warn!(error = %e, "embedding model not loaded; search is off");
            }
        }
    });
}

/// Starts the index queue and the folder watcher, which rescans every present folder.
/// Runs on a blocking thread.
fn start_indexing<R: Runtime>(
    app: &AppHandle<R>,
    db: Arc<storage::db::Db>,
    embedder: Arc<rag::embedder::BertEmbedder>,
) {
    use ingest::queue::{IndexQueue, QueueConfig, Services, PROGRESS_EVENT};
    use rag::embedder::Embedder;
    // A changed model clears every hash first, so the scan below reindexes everything.
    if let Err(e) = db.sync_embedding_model(embedder.model_id()) {
        tracing::warn!(error = %e, "embedding model check failed; indexing is off");
        return;
    }
    let svc = Services {
        db: db.clone(),
        registry: ingest::parser::ParserRegistry::full(&pdfium_path()),
        chunker: ingest::chunker::ChunkerConfig::default(),
        counter: embedder.clone(),
        embedder,
    };
    let emitter = app.clone();
    let queue = match IndexQueue::start(
        svc,
        QueueConfig::default(),
        tokio_util::sync::CancellationToken::new(),
        move |p| {
            desktop::set_tray_index(&emitter, Some(p));
            let _ = emitter.emit(PROGRESS_EVENT, p);
        },
    ) {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!(error = %e, "index queue did not start");
            return;
        }
    };
    let queue = Arc::new(queue);
    use ingest::scheduler::{self, Scheduler, SCHEDULE_EVENT};
    let (q, emitter) = (queue.clone(), app.clone());
    let scheduler = match Scheduler::start(
        scheduler::load(&db),
        scheduler::system_sources(),
        queue.budget().max(),
        move |current| {
            scheduler::apply(&q, current.plan);
            desktop::set_tray_schedule(&emitter, Some(current));
            let _ = emitter.emit(SCHEDULE_EVENT, current);
        },
    ) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::warn!(error = %e, "index scheduler did not start; indexing is off");
            return;
        }
    };
    let s = scheduler.clone();
    app.listen_any(desktop::FOCUS_EVENT, move |e| {
        s.set_searching(e.payload() == "true");
    });
    app.manage(scheduler);
    match ingest::watcher::FolderWatcher::start(db, queue.clone()) {
        Ok(w) => {
            app.manage(w);
        }
        // WatchError messages carry no paths.
        Err(e) => tracing::warn!(error = %e, "folder watcher did not start"),
    }
    // Windows that asked get_index_status before this point still show "not indexing".
    let _ = app.emit(PROGRESS_EVENT, queue.progress());
    desktop::set_tray_index(app, Some(queue.progress()));
    app.manage(queue);
}

/// The bundled PDFium: `Contents/Frameworks`, beside `Contents/MacOS/<exe>`. A debug build run
/// from `target/` falls back to the copy `scripts/fetch-pdfium.sh` put in `vendor/`.
fn pdfium_path() -> std::path::PathBuf {
    const LIB: &str = "libpdfium.dylib";
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("Frameworks").join(LIB)))
        .unwrap_or_default();
    #[cfg(debug_assertions)]
    if !bundled.exists() {
        return std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/pdfium/lib")
            .join(LIB);
    }
    bundled
}

fn open_storage(dir: &std::path::Path) -> Result<storage::db::Db, storage::error::StorageError> {
    std::fs::create_dir_all(dir)?;
    storage::db::open_or_create(
        &storage::db::db_path(dir),
        &storage::keychain::KeychainKeyProvider,
    )
}

/// Daily log files in `dir`, seven kept. Public for the privacy audit test.
pub fn init_tracing(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
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
    let Ok(app) = builder(tauri::Builder::default()).build(tauri::generate_context!()) else {
        // No error details: they can carry paths.
        eprintln!("startup failed");
        std::process::exit(1);
    };
    app.run(|_app, _event| {
        // With no Dock icon, opening the app again from Finder or Spotlight shows the main window.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            desktop::show_main(_app);
        }
    });
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

        assert_acl_denied(invoke_with(&main, "search", query()));
        for cmd in [
            "set_air_gap",
            "set_shortcut",
            "list_folders",
            "add_watched_folder",
            "remove_watched_folder",
            "set_folder_enabled",
            "list_failed_documents",
            "retry_document",
            "set_exclude_patterns",
            "get_schedule",
            "set_schedule",
            "set_supporter",
            "set_theme",
            "set_accent",
            "dismiss_sponsor_banner",
            "open_sponsor",
            "export_diagnostics",
            "delete_all_indexes",
            "get_about",
            "read_legal_text",
        ] {
            assert_acl_denied(invoke(&quick, cmd));
        }
        assert!(invoke(&quick, "get_index_status").is_ok());
        // Allowed: it fails only on the Db state this test does not manage.
        let err = invoke(&quick, "get_appearance").expect_err("no Db state");
        assert!(!err.to_string().contains("not allowed"), "got {err}");
        assert_acl_denied(invoke(&quick, "reveal_source"));
        for cmd in ["open_source", "copy_citation"] {
            assert_acl_denied(invoke(&main, cmd));
        }
        // The dialog plugin is registered, but no window may call it directly.
        assert_acl_denied(invoke(&main, "plugin:dialog|open"));
        assert_acl_denied(invoke(&quick, "plugin:dialog|open"));
        // Nor the updater plugin, whose own HTTP client would bypass `net::Client`.
        for cmd in ["check", "download", "install", "download_and_install"] {
            assert_acl_denied(invoke(&main, &format!("plugin:updater|{cmd}")));
            assert_acl_denied(invoke(&quick, &format!("plugin:updater|{cmd}")));
        }
        for cmd in [
            "list_models",
            "download_model",
            "cancel_model_download",
            "import_model_file",
            "check_for_update",
            "install_update",
        ] {
            assert_acl_denied(invoke(&quick, cmd));
        }
        assert_not_implemented(invoke(&main, "export_pack"));

        let (_dir, db) = storage::db::tests::temp_db();
        app.manage(Arc::new(db));
        let folders = invoke(&main, "list_folders").unwrap();
        assert_eq!(
            folders.deserialize::<serde_json::Value>().unwrap(),
            serde_json::json!([])
        );
        let id = || tauri::ipc::InvokeBody::Json(serde_json::json!({ "id": 7 }));
        let err = invoke_with(&main, "remove_watched_folder", id()).expect_err("unknown folder");
        assert_eq!(err["code"], "folder_not_found", "got {err}");
        let status = invoke(&main, "get_index_status").unwrap();
        let status = status.deserialize::<serde_json::Value>().unwrap();
        assert_eq!(status["indexing"], false);
        assert_eq!(status["progress"]["queued"], 0);

        let models_dir = tempfile::tempdir().unwrap();
        app.manage(rag::models::ModelManager::new(models_dir.path().into()));
        let guard =
            security::airgap::AirGapGuard::load(&app.state::<Arc<storage::db::Db>>()).unwrap();
        app.manage(net::Client::new(Arc::new(guard)).unwrap());
        let list = invoke(&main, "list_models").unwrap();
        let list = list.deserialize::<serde_json::Value>().unwrap();
        assert_eq!(list[0]["id"], rag::embedder::MODELS[0].id);
        assert_eq!(list[0]["installed"], false);
        let id = rag::embedder::MODELS[0].id;
        let body = |off: bool| {
            tauri::ipc::InvokeBody::Json(
                serde_json::json!({ "id": id, "airGapOffForDownload": off }),
            )
        };
        let err = invoke_with(&main, "download_model", body(false)).expect_err("air-gap is on");
        assert_eq!(err["code"], "air_gap_enabled", "got {err}");
        let unknown = tauri::ipc::InvokeBody::Json(
            serde_json::json!({ "id": "nope", "airGapOffForDownload": false }),
        );
        let err = invoke_with(&main, "download_model", unknown).expect_err("unknown id");
        assert_eq!(err["code"], "not_in_catalog", "got {err}");
        let err = invoke(&main, "check_for_update").expect_err("air-gap is on");
        assert_eq!(err["code"], "air_gap_enabled", "got {err}");
        let err = invoke(&main, "install_update").expect_err("nothing checked");
        assert_eq!(err["code"], "update_failed", "got {err}");
        assert!(app.state::<net::Client>().guard().is_enabled());
        assert_eq!(app.state::<net::Client>().sent_count(), 0);
    }

    #[test]
    fn the_app_registry_indexes_pdfs_and_images() {
        // v0.1.0 indexed only txt, md, and docx: the app never registered these parsers.
        let reg = ingest::parser::ParserRegistry::full(&pdfium_path());
        for ext in ["pdf", "png", "jpg", "jpeg", "txt", "md", "docx"] {
            assert!(reg.supports(ext), "{ext}");
        }
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
        app.manage(Arc::new(db));
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

        let search = |q: &str| {
            let body = tauri::ipc::InvokeBody::Json(serde_json::json!({ "query": q }));
            invoke_with(&quick, "search", body)
                .unwrap()
                .deserialize::<serde_json::Value>()
                .unwrap()
        };
        let res = search("\"launch date\" folder:notes ext:txt after:2000");
        assert_eq!(res["hits"][0]["fileName"], "plan.txt");
        assert_eq!(
            res["filters"],
            serde_json::json!([
                {"key": "folder", "value": "notes"},
                {"key": "ext", "value": "txt"},
                {"key": "after", "value": "2000"},
            ])
        );
        for q in [
            "launch folder:elsewhere",
            "launch type:pdf",
            "\"date launch\"",
        ] {
            assert_eq!(search(q)["hits"], serde_json::json!([]), "{q}");
        }
        search("ext:txt");
        let db = app.state::<Arc<storage::db::Db>>();
        let count: i64 = db
            .read(|c| {
                c.query_row(
                    "SELECT value FROM counters WHERE key = 'search_count'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        // Five searches with text; the operator-only one is not counted.
        assert_eq!(count, 5);
    }

    #[test]
    fn index_search_and_copy_make_no_network_call_with_air_gap_on() {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (_db_dir, db) = storage::db::tests::temp_db();
        let guard = security::airgap::AirGapGuard::load(&db).unwrap();
        assert!(guard.is_enabled(), "Air-Gap is ON by default");
        app.manage(net::Client::new(Arc::new(guard)).unwrap());
        app.manage(rag::hybrid::SearchService::new(
            Arc::new(rag::embedder::FakeEmbedder),
            Box::new(rag::hybrid::NoopReranker),
        ));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("plan.txt"),
            "The launch date moved to March.",
        )
        .unwrap();
        ingest::pipeline::index_folder_once(
            &db,
            &ingest::parser::ParserRegistry::new(),
            dir.path(),
            &ingest::chunker::ChunkerConfig::default(),
            &ingest::chunker::CharApprox,
            &rag::embedder::FakeEmbedder,
        )
        .unwrap();
        app.manage(Arc::new(db));

        let quick = window(&app, "quick-search");
        let body = tauri::ipc::InvokeBody::Json(serde_json::json!({ "query": "launch date" }));
        let res = invoke_with(&quick, "search", body)
            .unwrap()
            .deserialize::<serde_json::Value>()
            .unwrap();
        let chunk_id = res["hits"][0]["chunkId"].as_i64().expect("a hit");
        let body = tauri::ipc::InvokeBody::Json(serde_json::json!({ "chunkId": chunk_id }));
        invoke_with(&quick, "copy_citation", body).unwrap();
        let clip = std::process::Command::new("/usr/bin/pbpaste")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&clip.stdout).contains("plan.txt"));

        assert_eq!(app.state::<net::Client>().call_count(), 0);
    }

    /// An indexed `<tmp>/Notes/plan.txt` next to `<tmp>/outside/secret.txt`, in a mock app.
    fn indexed_app() -> (
        tauri::App<MockRuntime>,
        tempfile::TempDir,
        tempfile::TempDir,
        std::path::PathBuf,
    ) {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (db_dir, db) = storage::db::tests::temp_db();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(dir.path().join("outside")).unwrap();
        std::fs::write(root.join("plan.txt"), "The launch date moved to March.").unwrap();
        std::fs::write(dir.path().join("outside/secret.txt"), "secret").unwrap();
        ingest::pipeline::index_folder_once(
            &db,
            &ingest::parser::ParserRegistry::new(),
            &root,
            &ingest::chunker::ChunkerConfig::default(),
            &ingest::chunker::CharApprox,
            &rag::embedder::FakeEmbedder,
        )
        .unwrap();
        app.manage(Arc::new(db));
        let root = root.canonicalize().unwrap();
        (app, db_dir, dir, root)
    }

    fn sql(app: &tauri::App<MockRuntime>, stmt: &str) {
        app.state::<Arc<storage::db::Db>>()
            .write(|c| c.execute(stmt, []))
            .unwrap();
    }

    #[test]
    fn open_source_rejects_a_row_that_points_outside_its_folder() {
        let (app, _db_dir, _dir, root) = indexed_app();
        std::os::unix::fs::symlink(root.join("../outside/secret.txt"), root.join("link.txt"))
            .unwrap();
        let quick = window(&app, "quick-search");
        let chunk: i64 = app
            .state::<Arc<storage::db::Db>>()
            .read(|c| c.query_row("SELECT id FROM chunks", [], |r| r.get(0)))
            .unwrap();
        let outside = root.join("../outside/secret.txt").canonicalize().unwrap();
        // Each tampered row names a file that exists, so only the scope check stops it.
        for rel in [
            "../outside/secret.txt".to_owned(),
            outside.to_str().unwrap().to_owned(),
            "link.txt".to_owned(),
        ] {
            sql(&app, &format!("UPDATE documents SET rel_path = '{rel}'"));
            for mode in ["open", "reveal"] {
                let body = tauri::ipc::InvokeBody::Json(
                    serde_json::json!({ "chunkId": chunk, "mode": mode }),
                );
                let err = invoke_with(&quick, "open_source", body).expect_err("tampered row");
                assert_eq!(err["code"], "source_not_found", "{rel} {mode}: {err}");
            }
        }
        // A tampered folder root is just as outside: the document's path no longer resolves.
        sql(&app, "UPDATE documents SET rel_path = 'plan.txt'");
        sql(&app, "UPDATE folders SET path = '/nonexistent-root'");
        let body =
            tauri::ipc::InvokeBody::Json(serde_json::json!({ "chunkId": chunk, "mode": "open" }));
        let err = invoke_with(&quick, "open_source", body).expect_err("missing root");
        assert_eq!(err["code"], "source_not_found", "got {err}");
    }

    #[test]
    fn folder_panel_commands() {
        let (app, _db_dir, _dir, _root) = indexed_app();
        app.manage(
            net::Client::new(Arc::new(
                security::airgap::AirGapGuard::load(&app.state::<Arc<storage::db::Db>>()).unwrap(),
            ))
            .unwrap(),
        );
        app.manage(rag::hybrid::SearchService::new(
            Arc::new(rag::embedder::FakeEmbedder),
            Box::new(rag::hybrid::NoopReranker),
        ));
        let (main, quick) = (window(&app, "main"), window(&app, "quick-search"));
        let json = |v: serde_json::Value| tauri::ipc::InvokeBody::Json(v);
        let call = |w: &WebviewWindow<MockRuntime>, cmd: &str, v: serde_json::Value| {
            invoke_with(w, cmd, json(v))
                .unwrap()
                .deserialize::<serde_json::Value>()
                .unwrap()
        };
        let hits = || {
            call(&quick, "search", serde_json::json!({ "query": "launch" }))["hits"]
                .as_array()
                .unwrap()
                .len()
        };
        let folder = call(&main, "list_folders", serde_json::json!({}))[0].clone();
        assert_eq!(folder["enabled"], true);
        assert_eq!(hits(), 1);

        // Disabled: kept, but out of search; enabled again: back.
        for (on, expected) in [(false, 0), (true, 1)] {
            call(
                &main,
                "set_folder_enabled",
                serde_json::json!({ "id": folder["id"], "enabled": on }),
            );
            assert_eq!(
                call(&main, "list_folders", serde_json::json!({}))[0]["enabled"],
                on
            );
            assert_eq!(hits(), expected);
        }

        sql(
            &app,
            "UPDATE documents SET status = 'error', error_code = 'encrypted'",
        );
        let failed = call(
            &main,
            "list_failed_documents",
            serde_json::json!({ "folderId": folder["id"] }),
        );
        assert_eq!(failed[0]["relPath"], "plan.txt");
        assert_eq!(failed[0]["reason"], "encrypted");
        // Without the model there is no queue to retry with.
        let doc = failed[0]["docId"].clone();
        let err = invoke_with(
            &main,
            "retry_document",
            json(serde_json::json!({ "docId": doc })),
        )
        .expect_err("no queue");
        assert_eq!(err["code"], "model_missing", "got {err}");

        let settings = call(
            &main,
            "set_exclude_patterns",
            serde_json::json!({ "patterns": "drafts/\n*.tmp" }),
        );
        assert_eq!(settings["excludePatterns"], "drafts/\n*.tmp");
        assert_eq!(
            call(&main, "get_settings", serde_json::json!({}))["excludePatterns"],
            "drafts/\n*.tmp"
        );

        // No queue yet: the schedule is saved and read back, with no current state.
        let night = serde_json::json!({
            "mode": "paused",
            "allowedHours": { "startMinute": 1320, "endMinute": 420 },
            "lowPower": "one_worker",
        });
        let status = call(
            &main,
            "set_schedule",
            serde_json::json!({ "settings": night }),
        );
        assert_eq!(status["settings"], night);
        assert!(status["current"].is_null());
        assert_eq!(
            call(&main, "get_schedule", serde_json::json!({}))["settings"],
            night
        );
        let mut empty = night.clone();
        empty["allowedHours"]["endMinute"] = 1320.into();
        let err = invoke_with(
            &main,
            "set_schedule",
            json(serde_json::json!({ "settings": empty })),
        )
        .expect_err("empty range");
        assert_eq!(err["code"], "schedule_invalid", "got {err}");
        assert_eq!(
            call(&main, "get_schedule", serde_json::json!({}))["settings"],
            night
        );
    }

    #[test]
    fn set_air_gap_persists_and_get_settings_reads_it() {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (_dir, db) = storage::db::tests::temp_db();
        let guard = Arc::new(security::airgap::AirGapGuard::load(&db).unwrap());
        app.manage(net::Client::new(guard).unwrap());
        app.manage(Arc::new(db));
        let main = window(&app, "main");
        let net = app.state::<net::Client>();
        let db = app.state::<Arc<storage::db::Db>>();
        assert!(net.guard().is_enabled());

        for on in [false, true] {
            let body = tauri::ipc::InvokeBody::Json(serde_json::json!({ "enabled": on }));
            assert!(invoke_with(&main, "set_air_gap", body).is_ok());
            assert_eq!(net.guard().is_enabled(), on);
            let reloaded = security::airgap::AirGapGuard::load(&db).unwrap();
            assert_eq!(reloaded.is_enabled(), on);
            let settings = invoke(&main, "get_settings").unwrap();
            let settings = settings.deserialize::<serde_json::Value>().unwrap();
            assert_eq!(settings["airGap"], on);
            // No plugin in the mock app, so nothing is registered.
            assert_eq!(settings["shortcut"], desktop::DEFAULT_SHORTCUT);
            assert_eq!(settings["shortcutRegistered"], false);
        }
    }

    #[test]
    fn sponsor_banner_shows_once_and_air_gap_on_gives_only_a_qr() {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (_dir, db) = storage::db::tests::temp_db();
        let guard = Arc::new(security::airgap::AirGapGuard::load(&db).unwrap());
        app.manage(net::Client::new(guard).unwrap());
        app.manage(Arc::new(db));
        let main = window(&app, "main");
        let call = |cmd: &str, args: serde_json::Value| {
            invoke_with(&main, cmd, tauri::ipc::InvokeBody::Json(args))
                .unwrap()
                .deserialize::<serde_json::Value>()
                .unwrap()
        };
        let banner = || call("get_settings", serde_json::json!({}))["sponsorBanner"].clone();
        let db = app.state::<Arc<storage::db::Db>>();
        let net = app.state::<net::Client>();

        for _ in 0..99 {
            db.increment_search_count().unwrap();
        }
        assert_eq!(banner(), false);
        db.increment_search_count().unwrap();
        assert_eq!(banner(), true);
        assert_eq!(banner(), true, "stays until dismissed");
        let after = call("dismiss_sponsor_banner", serde_json::json!({}));
        assert_eq!(after["sponsorBanner"], false);
        for _ in 0..200 {
            db.increment_search_count().unwrap();
        }
        assert_eq!(banner(), false, "never shows again");

        // Air-Gap is on by default: a QR comes back, and no request is made.
        assert!(net.guard().is_enabled());
        for channel in 0..brand::SPONSOR_URLS.len() {
            let link = call("open_sponsor", serde_json::json!({ "channel": channel }));
            assert_eq!(link["kind"], "qr", "got {link}");
            assert_eq!(link["url"], brand::SPONSOR_URLS[channel]);
        }
        assert_eq!(net.call_count(), 0);

        // Themes are open without the supporter flag; the flag only adds the badge.
        let s = call("set_theme", serde_json::json!({ "theme": "terminal" }));
        assert_eq!(
            (s["theme"].as_str(), s["supporter"].as_bool()),
            (Some("terminal"), Some(false))
        );
        let s = call("set_supporter", serde_json::json!({ "on": true }));
        assert_eq!(s["supporter"], true);
        let s = call("set_supporter", serde_json::json!({ "on": false }));
        assert_eq!(
            (s["supporter"].as_bool(), s["theme"].as_str()),
            (Some(false), Some("terminal"))
        );

        let s = call("set_accent", serde_json::json!({ "accent": "brand" }));
        assert_eq!(s["accent"], "brand");
        let a = call("get_appearance", serde_json::json!({}));
        assert_eq!(
            (a["theme"].as_str(), a["accent"].as_str()),
            (Some("terminal"), Some("brand"))
        );
    }

    #[test]
    fn turning_supporter_on_hides_the_document_milestone_banner() {
        let app = builder(mock_builder())
            .build(tauri::generate_context!(test = true))
            .unwrap();
        let (_dir, db) = storage::db::tests::temp_db();
        let guard = Arc::new(security::airgap::AirGapGuard::load(&db).unwrap());
        app.manage(net::Client::new(guard).unwrap());
        app.manage(Arc::new(db));
        let main = window(&app, "main");
        let banner = |cmd: &str, args: serde_json::Value| {
            invoke_with(&main, cmd, tauri::ipc::InvokeBody::Json(args))
                .unwrap()
                .deserialize::<serde_json::Value>()
                .unwrap()["sponsorBanner"]
                .clone()
        };
        sql(
            &app,
            "INSERT INTO folders(id, path, added_at, status) VALUES (1, '/f', 0, 'ready')",
        );
        for i in 0..100 {
            sql(&app, &format!("INSERT INTO documents(folder_id, rel_path, ext, status) VALUES (1, 'd{i}', 'txt', 'indexed')"));
        }
        assert_eq!(banner("get_settings", serde_json::json!({})), true);
        assert_eq!(
            banner("set_supporter", serde_json::json!({ "on": true })),
            false
        );
        assert_eq!(
            banner("set_supporter", serde_json::json!({ "on": false })),
            false
        );
    }
}
