use core::{
    cell::RefCell,
    ffi::{CStr, c_char, c_void},
};

use super::{
    Store, dxmt_shader_cache_create, dxmt_shader_cache_destroy, dxmt_shader_cache_has_reader,
    dxmt_shader_cache_has_writer, dxmt_shader_cache_read, dxmt_shader_cache_write, path,
};

/// The handles the fakes hand out: the writer, the reader and a stored value.
const WRITER: u64 = 0x57;
const READER: u64 = 0x52;
const DATA: u64 = 0x44;

/// The C++ cache's recorded run: the matrix's lines, and what the stubs printed for each.
const GOLDEN_MATRIX: &str = include_str!("golden/matrix.txt");
const GOLDEN_CALLS: &str = include_str!("golden/calls.txt");

thread_local! {
    /// What the fakes were called with.
    ///
    /// A line per call, as the C++ golden's stubs print it. Each test runs on its own thread and
    /// starts by emptying it ([`start`]).
    static LOG: RefCell<String> = const { RefCell::new(String::new()) };
}

fn say(line: &str) {
    LOG.with_borrow_mut(|log| {
        log.push_str(line);
        log.push('\n');
    });
}

fn take() -> String {
    LOG.take()
}

/// Starts a test that reads the log: this thread's retains and releases go to it, and it is empty.
fn start() {
    crate::fakes::log_object_calls(object_call);
    take();
}

fn who(handle: u64) -> &'static str {
    match handle {
        WRITER => "writer",
        READER => "reader",
        DATA => "data",
        _ => "other",
    }
}

/// The text of a C path argument.
fn text(path: *const c_char) -> String {
    // SAFETY: the store passes the NUL-terminated path it built, alive for the call.
    unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .into_owned()
}

/// A path holding `/nowriter/` or `/noreader/` makes that side's open answer the null handle.
#[unsafe(export_name = "CacheWriter_alloc_init")]
extern "C-unwind" fn fake_writer_open(path: *const c_char, version: u64) -> u64 {
    let path = text(path);
    say(&format!(
        "CacheWriter_alloc_init path=\"{path}\" version={version:016x}"
    ));
    if path.contains("/nowriter/") {
        0
    } else {
        WRITER
    }
}

#[unsafe(export_name = "CacheReader_alloc_init")]
extern "C-unwind" fn fake_reader_open(path: *const c_char, version: u64) -> u64 {
    let path = text(path);
    say(&format!(
        "CacheReader_alloc_init path=\"{path}\" version={version:016x}"
    ));
    if path.contains("/noreader/") {
        0
    } else {
        READER
    }
}

/// A key starting with byte 1 is stored (answers `DATA`), any other is a miss.
#[unsafe(export_name = "CacheReader_get")]
extern "C-unwind" fn fake_get(reader: u64, key: *const c_void, length: u64) -> u64 {
    let len = usize::try_from(length).expect("a length that fits");
    // SAFETY: the store passes the key it was given, `length` bytes long.
    let key = unsafe { core::slice::from_raw_parts(key.cast::<u8>(), len) };
    say(&format!("CacheReader_get {} key={key:02x?}", who(reader)));
    if key.first() == Some(&1) { DATA } else { 0 }
}

#[unsafe(export_name = "CacheWriter_set")]
extern "C-unwind" fn fake_set(writer: u64, key: *const c_void, length: u64, value: u64) {
    let len = usize::try_from(length).expect("a length that fits");
    // SAFETY: the store passes the key it was given, `length` bytes long.
    let key = unsafe { core::slice::from_raw_parts(key.cast::<u8>(), len) };
    say(&format!(
        "CacheWriter_set {} key={key:02x?} value={}",
        who(writer),
        who(value)
    ));
}

/// The retains and releases of this thread's objects, from `crate::fakes`.
fn object_call(name: &'static str, object: u64) {
    say(&format!("{name} {}", who(object)));
}

/// A store the way the C++ holds one: behind the `void *` the exports take.
struct Raw(*mut c_void);

impl Raw {
    fn new(store: Store) -> Self {
        Self(Box::into_raw(Box::new(store)).cast::<c_void>())
    }

    fn has_reader(&self) -> bool {
        // SAFETY: `self.0` is a live store: boxed by `new` or `dxmt_shader_cache_create`'s, and
        // not destroyed (`destroy` consumes `self`).
        unsafe { dxmt_shader_cache_has_reader(self.0) }
    }

    fn has_writer(&self) -> bool {
        // SAFETY: as above.
        unsafe { dxmt_shader_cache_has_writer(self.0) }
    }

    fn write(&self, key: &[u8], data: u64) {
        let len = key.len() as u64;
        // SAFETY: as above; `key` is readable for its length, `data` is a handle.
        unsafe { dxmt_shader_cache_write(self.0, key.as_ptr().cast(), len, data) };
    }

    fn read(&self, key: &[u8]) -> u64 {
        let len = key.len() as u64;
        // SAFETY: as above; `key` is readable for its length.
        unsafe { dxmt_shader_cache_read(self.0, key.as_ptr().cast(), len) }
    }

    fn destroy(self) {
        // SAFETY: as above; the store is given back once.
        unsafe { dxmt_shader_cache_destroy(self.0) };
    }
}

/// A store as the C++ makes one, on a path, with the version the C++ passes.
fn open(path: &str) -> Store {
    Store::open(Some(path), 7)
}

#[test]
fn path_strings_are_the_cpps() {
    // The native exe name is empty (the C++'s `getExePath` is a TODO there).
    assert_eq!(
        path(None, None, "", 310).as_deref(),
        Some("dxmt//shaders_310.db")
    );
    assert_eq!(
        path(None, None, "Game.exe", 310).as_deref(),
        Some("dxmt/Game.exe/shaders_310.db")
    );
    // An absolute value gets its slash once.
    for value in ["/x/cache", "/x/cache/"] {
        assert_eq!(
            path(None, Some(value), "Game.exe", 310).as_deref(),
            Some("/x/cache/shaders_310.db"),
            "{value}"
        );
    }
    assert_eq!(
        path(None, Some("/"), "g", 310).as_deref(),
        Some("/shaders_310.db")
    );
    // An empty or relative value falls back to the exe's folder.
    for value in ["", "rel", "rel/", "./x", "C:/x"] {
        assert_eq!(
            path(None, Some(value), "Game.exe", 320).as_deref(),
            Some("dxmt/Game.exe/shaders_320.db"),
            "{value:?}"
        );
    }
    // A different cache switch than "0" leaves it on.
    for value in ["1", "", "00", "0 ", "off"] {
        assert!(path(Some(value), None, "g", 310).is_some(), "{value:?}");
    }
}

#[test]
fn zero_turns_it_off() {
    start();
    assert_eq!(path(Some("0"), Some("/x"), "Game.exe", 310), None);
    let off = Store::open(None, 7);
    assert!(!off.has_reader() && !off.has_writer());
    assert_eq!(off.read(&[1]).map(|data| data.handle()), None);
    drop(off);
    assert_eq!(take(), "", "an off cache calls nothing");
}

#[test]
fn file_is_named_by_metal_version() {
    for (version, name) in [
        (310, "/x/shaders_310.db"),
        (320, "/x/shaders_320.db"),
        (0, "/x/shaders_0.db"),
        (u32::MAX, "/x/shaders_4294967295.db"),
    ] {
        assert_eq!(
            path(None, Some("/x"), "Game.exe", version).as_deref(),
            Some(name)
        );
    }
}

#[test]
fn writer_opens_before_reader() {
    start();
    let store = open("/x/shaders_310.db");
    assert_eq!(
        take(),
        "CacheWriter_alloc_init path=\"/x/shaders_310.db\" version=0000000000000007\n\
         CacheReader_alloc_init path=\"/x/shaders_310.db\" version=0000000000000007\n"
    );
    drop(store);
}

#[test]
fn drop_releases_reader_and_writer() {
    start();
    let store = open("/x/shaders_310.db");
    take();
    drop(store);
    assert_eq!(take(), "NSObject_release reader\nNSObject_release writer\n");
}

#[test]
fn a_side_that_failed_to_open_is_off() {
    start();
    let cold = open("/noreader/shaders_310.db");
    assert!(cold.has_writer() && !cold.has_reader());
    assert!(cold.read(&[1]).is_none());
    take();
    drop(cold);
    assert_eq!(take(), "NSObject_release writer\n");
    let broken = open("/nowriter/shaders_310.db");
    assert!(!broken.has_writer() && broken.has_reader());
}

#[test]
fn cache_golden_replays() {
    start();
    let (head, rows) = GOLDEN_CALLS.split_once('\n').expect("a version line");
    let version = head
        .strip_prefix("ShaderCacheVersion ")
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .expect("the C++ cache version");
    let rows = rows.split("== singleton").next().expect("the rows");
    say(head);
    let mut row = 0;
    for line in GOLDEN_MATRIX
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let field = |i: usize| match line.split('|').nth(i).expect("four fields") {
            "<unset>" => None,
            "<empty>" => Some(""),
            value => Some(value),
        };
        let metal_version: u32 = line
            .rsplit('|')
            .next()
            .and_then(|v| v.parse().ok())
            .expect("a version");
        row += 1;
        say(&format!("== row {row}: {line}"));
        // Natively the exe name is empty, which is what the golden's unset rows recorded.
        let file = path(field(2), field(0), field(1).unwrap_or(""), metal_version);
        let store = Store::open(file.as_deref(), version);
        say(&format!(
            "   writer {} reader {}",
            if store.has_writer() { "open" } else { "none" },
            if store.has_reader() { "open" } else { "none" }
        ));
        drop(store);
        say("-- destroyed");
    }
    let replay = take();
    let replay = replay
        .strip_prefix(&format!("{head}\n"))
        .expect("the version line");
    assert_eq!(replay, rows);
    assert_eq!(row, 84, "the matrix's rows");
}

#[test]
fn exports_pass_keys_and_handles() {
    start();
    let store = Raw::new(open("/x/shaders_310.db"));
    take();
    assert!(store.has_reader() && store.has_writer());
    store.write(&[1, 2, 3], DATA);
    assert_eq!(store.read(&[1, 2, 3]), DATA);
    assert_eq!(store.read(&[9, 2, 3]), 0);
    // A stored value is handed over with its reference (no release here); a value to write is lent.
    assert_eq!(
        take(),
        "CacheWriter_set writer key=[01, 02, 03] value=data\n\
         CacheReader_get reader key=[01, 02, 03]\n\
         CacheReader_get reader key=[09, 02, 03]\n"
    );
    store.destroy();
    assert_eq!(take(), "NSObject_release reader\nNSObject_release writer\n");
}

#[test]
fn exports_of_an_off_store_do_nothing() {
    start();
    let store = Raw::new(Store::open(None, 7));
    assert!(!store.has_reader() && !store.has_writer());
    store.write(&[1], DATA);
    assert_eq!(store.read(&[1]), 0);
    store.destroy();
    assert_eq!(take(), "");
}

#[test]
fn create_and_destroy_pair_up() {
    start();
    // The store is destroyed below.
    let store = Raw(dxmt_shader_cache_create(310, 7));
    assert!(!store.0.is_null());
    // Whatever the environment says, both sides are on or both off.
    assert_eq!(store.has_reader(), store.has_writer());
    store.destroy();
}
