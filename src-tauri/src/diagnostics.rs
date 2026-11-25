// SPDX-License-Identifier: GPL-3.0-or-later
//! Local crash reports and the "Export diagnostics" zip. Nothing here sends anything: a report
//! stays in the app's log folder until the user exports a zip and shares it themselves.

pub mod error;

use std::backtrace::Backtrace;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;

use crate::storage::db::Db;
use error::DiagnosticsError;

/// Crash reports live here, inside the app's log folder.
pub const CRASH_DIR: &str = "crashes";

/// Writes `crash-<unix seconds>-<pid>.txt` into `dir` on every panic.
/// The panic message is left out: it can quote a path or document text (a failed string
/// slice prints the string). Only the code location, thread name, and symbol names remain.
pub fn install_panic_hook(dir: PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        let report = crash_report(
            info.location(),
            std::thread::current().name(),
            &Backtrace::force_capture(),
        );
        let location = info.location().map(code_location).unwrap_or_default();
        tracing::error!(%location, "panic; crash report saved");
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("crash-{secs}-{}.txt", std::process::id());
        let _ = std::fs::write(dir.join(name), report);
    }));
}

/// `file.rs:line`, without the build machine's folders.
fn code_location(l: &std::panic::Location<'_>) -> String {
    let file = l.file().rsplit(['/', '\\']).next().unwrap_or("");
    format!("{file}:{}", l.line())
}

fn crash_report(
    location: Option<&std::panic::Location<'_>>,
    thread: Option<&str>,
    backtrace: &Backtrace,
) -> String {
    let mut r = format!(
        "{} {} crash report\nsource: {}\nos: {} {}\nthread: {}\nlocation: {}\nbacktrace:\n",
        crate::brand::APP_NAME,
        env!("CARGO_PKG_VERSION"),
        crate::about::SOURCE_REV,
        std::env::consts::OS,
        std::env::consts::ARCH,
        thread.unwrap_or("unnamed"),
        location.map(code_location).unwrap_or_default(),
    );
    // Frame lines only; the `at <file>` lines name folders on the build machine.
    for line in backtrace.to_string().lines() {
        if !line.trim_start().starts_with("at ") {
            r += line;
            r += "\n";
        }
    }
    r
}

/// What the summary in an export reads from the running app.
pub struct AppState<'a> {
    pub db: Option<&'a Db>,
    pub air_gap: bool,
    pub installed_models: Vec<&'static str>,
}

/// Writes a zip to `dest`: `summary.txt` (version, OS, counts by status and failure reason),
/// every file in `log_dir` under `logs/`, and every crash report under `crashes/`.
/// Never includes document text, paths, file names, or queries: the logs and crash reports
/// hold none (absolute rule 5), and the summary holds only counts.
pub fn export(dest: &Path, log_dir: &Path, state: &AppState<'_>) -> Result<(), DiagnosticsError> {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(dest)?);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("summary.txt", opts)?;
    zip.write_all(summary(state)?.as_bytes())?;
    for (dir, prefix) in [
        (log_dir.to_path_buf(), "logs"),
        (log_dir.join(CRASH_DIR), "crashes"),
    ] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .collect();
        files.sort_by_key(|e| e.file_name());
        for e in files {
            // Our own file names: `<slug>.<date>.log` and `crash-<secs>-<pid>.txt`.
            let name = e.file_name().to_string_lossy().into_owned();
            zip.start_file(format!("{prefix}/{name}"), opts)?;
            std::io::copy(&mut std::fs::File::open(e.path())?, &mut zip)?;
        }
    }
    zip.finish()?;
    Ok(())
}

fn summary(state: &AppState<'_>) -> Result<String, DiagnosticsError> {
    let mut s = format!(
        "app: {} {}\nsource: {}\nos: {} {}\ncores: {}\nair_gap: {}\nmodels: {}\n",
        crate::brand::APP_NAME,
        env!("CARGO_PKG_VERSION"),
        crate::about::SOURCE_REV,
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        state.air_gap,
        state.installed_models.join(", "),
    );
    let Some(db) = state.db else {
        s += "storage: not open\n";
        return Ok(s);
    };
    let rows: Vec<(String, Option<String>, i64)> = db.read(|c| {
        let mut st = c.prepare(
            "SELECT status, error_code, count(*) FROM documents GROUP BY 1, 2 ORDER BY 1, 2",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    })?;
    let (schema, folders, chunks): (i64, i64, i64) = db.read(|c| {
        c.query_row(
            "SELECT (SELECT user_version FROM pragma_user_version),
                    (SELECT count(*) FROM folders), (SELECT count(*) FROM chunks)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    })?;
    s += &format!("schema: {schema}\nfolders: {folders}\nchunks: {chunks}\n");
    for (status, code, n) in rows {
        s += &format!("documents {status} {}: {n}\n", code.unwrap_or_default());
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn entries(zip: &Path) -> Vec<(String, String)> {
        let mut a = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        (0..a.len())
            .map(|i| {
                let mut f = a.by_index(i).unwrap();
                let mut s = String::new();
                f.read_to_string(&mut s).unwrap();
                (f.name().unwrap().into_owned(), s)
            })
            .collect()
    }

    #[test]
    fn crash_report_drops_the_message_and_build_folders() {
        let loc = std::panic::Location::caller();
        let r = crash_report(Some(loc), Some("index-worker"), &Backtrace::force_capture());
        assert!(r.contains(&format!("location: diagnostics.rs:{}", loc.line())));
        assert!(r.contains("thread: index-worker"));
        assert!(!r.contains(env!("CARGO_MANIFEST_DIR")), "{r}");
        assert!(r
            .lines()
            .all(|l| !l.trim_start().starts_with("at ") && !l.contains("/src/")));
    }

    #[test]
    fn export_holds_summary_logs_and_crashes_and_only_counts() {
        let (_d, db) = crate::storage::db::tests::temp_db();
        db.write(|c| {
            c.execute_batch(
                "INSERT INTO folders(id, path, added_at, status) VALUES (1, '/secret-folder', 0, 'ready');
                 INSERT INTO documents(folder_id, rel_path, ext, status) VALUES (1, 'secret-name.txt', 'txt', 'indexed');
                 INSERT INTO documents(folder_id, rel_path, ext, status, error_code) VALUES (1, 'b.pdf', 'pdf', 'error', 'encrypted');",
            )
        })
        .unwrap();
        let logs = tempfile::tempdir().unwrap();
        let log = format!("{}.2026-11-25.log", crate::brand::APP_SLUG);
        std::fs::write(logs.path().join(&log), "app started\n").unwrap();
        std::fs::create_dir(logs.path().join(CRASH_DIR)).unwrap();
        std::fs::write(logs.path().join(CRASH_DIR).join("crash-1-2.txt"), "boom").unwrap();
        let out = tempfile::tempdir().unwrap();
        let zip = out.path().join("d.zip");
        let state = AppState {
            db: Some(&db),
            air_gap: true,
            installed_models: vec!["bge-small-en-v1.5"],
        };
        export(&zip, logs.path(), &state).unwrap();

        let e = entries(&zip);
        let names: Vec<&str> = e.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "summary.txt".to_owned(),
                format!("logs/{log}"),
                "crashes/crash-1-2.txt".to_owned()
            ]
        );
        let summary = &e[0].1;
        for want in [
            "air_gap: true",
            "folders: 1",
            "documents indexed : 1",
            "documents error encrypted: 1",
        ] {
            assert!(summary.contains(want), "{want} missing in {summary}");
        }
        assert!(!summary.contains("secret"));
        assert_eq!(e[2].1, "boom");
    }
}
