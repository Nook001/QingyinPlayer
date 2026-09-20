use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use qingyin_library::{scan_directory, watch_directories};
use qingyin_storage::Database;
use rusqlite::Connection;

fn main() {
    let directory = PathBuf::from(env::args().nth(1).expect("usage: stress_bench DIRECTORY"));
    let work = env::temp_dir().join(format!(
        "qingyin-stress-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&work).expect("work dir");
    let db_path = work.join("library.sqlite3");
    let mut database = Database::open(&db_path).expect("open");

    let import = Instant::now();
    let first = scan_directory(&mut database, &directory).expect("import");
    let import_ms = import.elapsed().as_secs_f64() * 1000.0;
    drop(database);

    let lock_started = Instant::now();
    let locker = Connection::open(&db_path).expect("locker");
    locker
        .execute_batch("BEGIN EXCLUSIVE")
        .expect("exclusive lock");
    let locked_result: Result<(), String> = match Database::open(&db_path) {
        Ok(mut database) => scan_directory(&mut database, &directory)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    let lock_ms = lock_started.elapsed().as_secs_f64() * 1000.0;
    locker.execute_batch("ROLLBACK").ok();
    drop(locker);

    let mut database = Database::open(&db_path).expect("reopen");
    let after_lock = scan_directory(&mut database, &directory).expect("after lock");
    let tracks = database.list_tracks().expect("list").len();
    drop(database);

    let missing = work.join("offline-root");
    let (tx, rx) = mpsc::channel();
    let watcher = watch_directories(vec![missing.clone()], db_path.clone(), move |snapshot| {
        let _ = tx.send(snapshot);
    })
    .expect("watch offline");
    fs::create_dir_all(&missing).expect("remount");
    let remount = rx.recv_timeout(Duration::from_secs(6));
    let stop = Instant::now();
    drop(watcher);
    let stop_ms = stop.elapsed().as_secs_f64() * 1000.0;

    println!(
        "import_ms={import_ms:.2} imported={} lock_ms={lock_ms:.2} lock_err={} after_lock_unchanged={} tracks={tracks} remount_reconcile={} stop_ms={stop_ms:.2}",
        first.imported,
        locked_result.err().unwrap_or_default(),
        after_lock.unchanged,
        remount
            .as_ref()
            .ok()
            .is_some_and(|snapshot| snapshot.summary.needs_reconcile)
    );
    let _ = fs::remove_dir_all(&work);
}
