use std::ffi::CString;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use objc2::runtime::AnyClass;

use super::*;
use crate::ffi::Handler;

/// What the probe fills an out-structure with: a handler that writes nothing leaves it.
const FILL: u64 = 0xA5A5_A5A5_A5A5_A5A5;

/// Set in the child [`the_classes_carry_the_names_of_this_build`] runs: it checks them at load.
const CHILD_CLASSES: &str = "WINEMETAL_TEST_CACHE_CLASSES_AT_LOAD";

/// Set in the child [`wrong_class_handle_raises`] runs: it sends a cache message to a non-cache.
const CHILD: &str = "WINEMETAL_TEST_CACHE_WRONG_CLASS";

/// A folder of this process's, empty.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("winemetal-g8-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn mem(p: *const u8) -> MemPtr {
    MemPtr { ptr: p.expose_provenance() as u64 }
}

fn call<T>(handler: Handler, params: &mut T) {
    // SAFETY: a live params block of the slot's layout.
    let status = unsafe { handler(ptr::from_mut(params).cast()) };
    assert_eq!(status, 0);
}

/// Slot 115 or 117 on `path`: the object's handle, 0 when the open failed.
fn open(handler: Handler, path: &Path, version: u64) -> u64 {
    let path = CString::new(path.to_str().expect("a UTF-8 path")).expect("no NUL");
    let mut params = UnixcallCacheAllocInit { path: mem(path.as_ptr().cast()), version, ret_cache: FILL };
    call(handler, &mut params);
    params.ret_cache
}

fn writer(path: &Path, version: u64) -> u64 {
    open(cache_writer_alloc_init, path, version)
}

fn reader(path: &Path, version: u64) -> u64 {
    open(cache_reader_alloc_init, path, version)
}

/// Gives back the +1 an open handler returned.
fn release(handle: u64) {
    let object = ptr::with_exposed_provenance_mut::<AnyObject>(u64_to_usize(handle));
    // SAFETY: a +1 object the open handed out.
    drop(unsafe { Retained::from_raw(object) });
}

/// Slot 118.
fn put(writer: u64, key: &[u8], value: &[u8]) {
    let value = DispatchData::from_bytes(value);
    let mut params = UnixcallCacheSet {
        cache: writer,
        key: mem(key.as_ptr()),
        key_length: key.len() as u64,
        value_data: ptr::from_ref::<DispatchData>(&value).expose_provenance() as u64,
    };
    call(cache_writer_set, &mut params);
}

/// Slot 116: the value, `None` for a miss.
fn get(reader: u64, key: &[u8]) -> Option<Vec<u8>> {
    let mut params =
        UnixcallCacheGet { cache: reader, key: mem(key.as_ptr()), key_length: key.len() as u64, ret_data: FILL };
    call(cache_reader_get, &mut params);
    let data = ptr::with_exposed_provenance_mut::<DispatchData>(u64_to_usize(params.ret_data));
    // SAFETY: the +1 dispatch data object the lookup made, or null.
    let data = NonNull::new(data).map(|data| unsafe { DispatchRetained::from_raw(data) });
    data.map(|data| data.to_vec())
}

/// The rows `sql` selects from the database at `path`, columns joined by `/`.
fn rows(path: &Path, sql: &CStr) -> Vec<String> {
    let path = CString::new(path.to_str().expect("a UTF-8 path")).expect("no NUL");
    let db = Db::open(&path, sqlite::OPEN_READWRITE).expect("the database opens");
    let stmt = db.prepare(sql).expect("the query prepares");
    let mut out = Vec::new();
    while stmt.step() == sqlite::ROW {
        let mut columns = Vec::new();
        for column in 0.. {
            let Some(text) = stmt.column_text(column) else { break };
            columns.push(text.to_string_lossy().into_owned());
        }
        out.push(columns.join("/"));
    }
    out
}

fn exec(path: &Path, sql: &CStr) {
    let path = CString::new(path.to_str().expect("a UTF-8 path")).expect("no NUL");
    Db::open(&path, sqlite::OPEN_READWRITE | sqlite::OPEN_CREATE)
        .expect("the database opens")
        .exec(sql)
        .expect("the statement runs");
}

#[test]
fn table_name_is_per_version() {
    assert_eq!(table_name(1), "cache_1");
    assert_eq!(table_name(u64::MAX), "cache_18446744073709551615");
}

#[test]
fn round_trip_through_the_handlers() {
    let db = folder("round-trip").join("cache.db");
    let w = writer(&db, 1);
    assert_ne!(w, 0);
    assert_ne!(w, FILL);
    assert!(db.exists());
    assert!(db.with_extension("db-lock").exists());
    put(w, b"key-0001", b"first value");
    put(w, b"key-0002", &[7; 3000]);
    put(w, b"key-0001", b"replaced");
    // The writer is open (WAL): the reader sees its writes.
    let r = reader(&db, 1);
    assert_ne!(r, 0);
    assert_eq!(get(r, b"key-0001").as_deref(), Some(&b"replaced"[..]));
    assert_eq!(get(r, b"key-0002").as_deref(), Some(&[7; 3000][..]));
    assert_eq!(get(r, b"key-0009"), None);
    // A key is its exact bytes: a prefix and an extension miss.
    assert_eq!(get(r, b"key-000"), None);
    assert_eq!(get(r, b"key-00011"), None);
    assert_eq!(rows(&db, c"SELECT count(*) FROM cache_1"), ["2"]);
    release(r);
    release(w);
    let _ = std::fs::remove_dir_all(db.parent().expect("a folder"));
}

#[test]
fn an_empty_key_is_null_and_an_empty_value_is_refused() {
    let db = folder("edges").join("cache.db");
    let w = writer(&db, 1);
    put(w, b"", b"under the empty key");
    put(w, b"key-0001", b"");
    put(w, b"key-0002", b"kept");
    release(w);
    // A NULL key (an empty NSData's bytes were NULL); the empty value never made a row.
    assert_eq!(rows(&db, c"SELECT typeof(key), length(value) FROM cache_1 ORDER BY rowid"), ["null/19", "blob/4"]);
    let r = reader(&db, 1);
    assert_eq!(get(r, b""), None);
    assert_eq!(get(r, b"key-0001"), None);
    assert_eq!(get(r, b"key-0002").as_deref(), Some(&b"kept"[..]));
    release(r);
    let _ = std::fs::remove_dir_all(db.parent().expect("a folder"));
}

#[test]
fn a_new_version_drops_the_other_cache_tables_and_vacuums() {
    let db = folder("migrate").join("cache.db");
    let w = writer(&db, 1);
    for i in 0..40_u8 {
        put(w, &[b'k', i], &[i; 2048]);
    }
    release(w);
    exec(
        &db,
        c"CREATE TABLE other(x); CREATE TABLE cachefoo(x); CREATE TABLE Cache_upper(x);
          CREATE TABLE cache_9(key BLOB PRIMARY KEY, value BLOB NOT NULL); CREATE TABLE cache_3(x);",
    );
    let pages = rows(&db, c"PRAGMA page_count")[0].parse::<u32>().expect("a count");
    let w = writer(&db, 2);
    assert_ne!(w, 0);
    release(w);
    // GLOB is case-sensitive; only `cache_*` tables of another version go.
    assert_eq!(
        rows(&db, c"SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"),
        ["Cache_upper", "cache_2", "cachefoo", "other"]
    );
    // VACUUM ran: nothing is left on the freelist, and the file shrank.
    assert_eq!(rows(&db, c"PRAGMA freelist_count"), ["0"]);
    assert!(rows(&db, c"PRAGMA page_count")[0].parse::<u32>().expect("a count") < pages);
    assert_eq!(reader(&db, 1), 0);
    // With no table to drop, a writer leaves the file's freelist alone.
    let w = writer(&db, 2);
    for i in 0..40_u8 {
        put(w, &[b'k', i], &[i; 2048]);
    }
    release(w);
    exec(&db, c"DELETE FROM cache_2");
    let freed = rows(&db, c"PRAGMA freelist_count");
    assert_ne!(freed, ["0"]);
    release(writer(&db, 2));
    assert_eq!(rows(&db, c"PRAGMA freelist_count"), freed);
    let _ = std::fs::remove_dir_all(db.parent().expect("a folder"));
}

#[test]
fn bad_paths_answer_nil() {
    let dir = folder("bad");
    std::fs::create_dir_all(&dir).expect("a folder");
    std::fs::write(dir.join("text.db"), "this is not a database file at all").expect("a file");
    std::fs::create_dir(dir.join("dir.db")).expect("a folder");
    let unrelated = dir.join("unrelated.db");
    exec(&unrelated, c"CREATE TABLE unrelated(x)");
    for path in [
        Path::new("/dev/null/x/cache.db").to_path_buf(),
        dir.join("missing.db"),
        dir.join("text.db"),
        dir.join("dir.db"),
        unrelated,
    ] {
        assert_eq!(reader(&path, 1), 0, "reader {}", path.display());
    }
    // A reader never creates the file.
    assert!(!dir.join("missing.db").exists());
    // An absolute path is used as is: a trailing slash names no file.
    for path in [Path::new("/dev/null/x/cache.db").to_path_buf(), dir.join("text.db"), dir.join("dir.db"), dir.join("trail/")] {
        assert_eq!(writer(&path, 1), 0, "writer {}", path.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_path_that_is_not_utf8_or_null_is_nil() {
    assert!(utf8_string(MemPtr { ptr: 0 }).is_none());
    assert!(utf8_string(mem(c"\xff\xfe".as_ptr().cast())).is_none());
    assert_eq!(utf8_string(mem(c"/tmp/a".as_ptr().cast())).expect("valid UTF-8").to_string(), "/tmp/a");
}

#[test]
fn a_writer_waits_for_the_lock_file() {
    let dir = folder("lock");
    let db = dir.join("cache.db");
    release(writer(&db, 1));
    let lock = db.with_extension("db-lock");
    assert!(lock.exists());
    let path = CString::new(lock.to_str().expect("a UTF-8 path")).expect("no NUL");
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        // SAFETY: a NUL-terminated path.
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o666_u32) };
        // SAFETY: an open descriptor.
        assert_eq!(unsafe { libc::flock(fd, libc::LOCK_EX) }, 0);
        held_tx.send(()).expect("the main thread waits");
        std::thread::sleep(Duration::from_millis(300));
        let released = Instant::now();
        // SAFETY: the descriptor opened above.
        unsafe { libc::flock(fd, libc::LOCK_UN) };
        // SAFETY: as above, closed once.
        unsafe { libc::close(fd) };
        released
    });
    held_rx.recv().expect("the holder holds the lock");
    let w = writer(&db, 1);
    let opened = Instant::now();
    assert_ne!(w, 0);
    // The open returned only after the holder let go.
    assert!(opened >= holder.join().expect("the holder ends"));
    release(w);
    // And the writer gave the lock up: it can be taken at once.
    let path = CString::new(lock.to_str().expect("a UTF-8 path")).expect("no NUL");
    let again = FileLock::exclusive(&path).expect("the lock file opens");
    drop(again);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_relative_path_is_under_the_user_cache_folder() {
    let unique = format!("winemetal-g8-relative-{}", std::process::id());
    let mut buf = [0 as c_char; PATH_MAX];
    // SAFETY: a buffer of its own length.
    assert_ne!(unsafe { libc::confstr(libc::_CS_DARWIN_USER_CACHE_DIR, buf.as_mut_ptr(), buf.len()) }, 0);
    // SAFETY: `confstr` NUL-terminated it.
    let base = PathBuf::from(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().expect("a UTF-8 folder"));
    let top = base.join(&unique);
    let _ = std::fs::remove_dir_all(&top);
    // The path is joined under the folder as a component: doubled and trailing slashes drop.
    for (relative, at) in [("a/b/cache.db", "a/b/cache.db"), ("dbl//cache.db", "dbl/cache.db"), ("trail/", "trail")] {
        let w = writer(Path::new(&format!("{unique}/{relative}")), 1);
        assert_ne!(w, 0, "{relative}");
        release(w);
        assert!(top.join(at).exists(), "{relative}");
    }
    let _ = std::fs::remove_dir_all(&top);
}

#[test]
fn absolute_paths_are_used_as_they_are() {
    let dir = folder("absolute");
    let path = NSString::from_str(dir.join("a/b/cache.db").to_str().expect("a UTF-8 path"));
    let resolved = resolve_cache_dir(Some(&path), true).expect("resolved");
    assert_eq!(resolved.to_string(), path.to_string());
    // The file's folder was made; the file was not.
    assert!(dir.join("a/b").is_dir());
    assert!(!dir.join("a/b/cache.db").exists());
    // A folder path makes the folder itself.
    let path = NSString::from_str(dir.join("c/d").to_str().expect("a UTF-8 path"));
    resolve_cache_dir(Some(&path), false).expect("resolved");
    assert!(dir.join("c/d").is_dir());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_nil_handle_is_a_nil_send() {
    let mut get_params = UnixcallCacheGet { cache: 0, key: mem(b"key-0001".as_ptr()), key_length: 8, ret_data: FILL };
    call(cache_reader_get, &mut get_params);
    assert_eq!(get_params.ret_data, 0);
    let mut set_params = UnixcallCacheSet { cache: 0, key: mem(b"key-0001".as_ptr()), key_length: 8, value_data: FILL };
    call(cache_writer_set, &mut set_params);
    assert_eq!(set_params.value_data, FILL);
    assert_eq!(set_params.cache, 0);
}

#[test]
fn metal_cache_path_answers_whether_metal_took_it() {
    let dir = folder("metal");
    let path = CString::new(dir.to_str().expect("a UTF-8 path")).expect("no NUL");
    let mut params = UnixcallSetmetalcachepath { path: mem(path.as_ptr().cast()), ret_success: FILL };
    call(set_metal_shader_cache_path, &mut params);
    // The folder is made whatever Metal answers; the answer is 0 or 1.
    assert!(dir.is_dir());
    assert!(params.ret_success <= 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The cache classes' names in this build, then the other build's.
fn class_names() -> ([&'static str; 2], [&'static str; 2]) {
    if cfg!(feature = "native") {
        (["DXMTNativeCacheReader", "DXMTNativeCacheWriter"], ["CacheReader", "CacheWriter"])
    } else {
        (["CacheReader", "CacheWriter"], ["DXMTNativeCacheReader", "DXMTNativeCacheWriter"])
    }
}

fn class_exists(name: &str) -> bool {
    AnyClass::get(&CString::new(name).expect("no NUL")).is_some()
}

#[test]
fn the_classes_carry_the_names_of_this_build() {
    let (this, other) = class_names();
    if std::env::var_os(CHILD_CLASSES).is_some() {
        // The only test this process runs: nothing but the image's constructor made the classes.
        assert!(this.iter().all(|name| class_exists(name)));
        assert!(!other.iter().any(|name| class_exists(name)));
        return;
    }
    let status = Command::new(std::env::current_exe().expect("the test binary's path"))
        .args(["--exact", "g8::tests::the_classes_carry_the_names_of_this_build", "--test-threads=1"])
        .env(CHILD_CLASSES, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the test binary runs again");
    assert!(status.success(), "classes missing at load: {status}");
    let dir = folder("class-names");
    let w = writer(&dir.join("cache.db"), 1);
    // SAFETY: a live object.
    let class = unsafe { handle::obj::<NSObject>(w) }.expect("an object").class();
    assert_eq!(class.name().to_str().expect("a UTF-8 name"), this[1]);
    release(w);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wrong_class_handle_raises() {
    if std::env::var_os(CHILD).is_some() {
        let object = NSObject::new();
        let mut params = UnixcallCacheGet {
            cache: ptr::from_ref::<NSObject>(&object).expose_provenance() as u64,
            key: mem(b"key".as_ptr()),
            key_length: 3,
            ret_data: FILL,
        };
        call(cache_reader_get, &mut params);
        return;
    }
    let output = Command::new(std::env::current_exe().expect("the test binary's path"))
        .args(["--exact", "g8::tests::wrong_class_handle_raises", "--test-threads=1", "--nocapture"])
        .env(CHILD, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("the test binary runs again");
    // The Objective-C exception leaves the handler (it is no Rust panic, so the guard does not
    // abort) and reaches the test harness's frame, which cannot catch it and aborts the child.
    assert_eq!(output.status.signal(), Some(libc::SIGABRT), "{}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot catch foreign exceptions"), "{stderr}");
}
