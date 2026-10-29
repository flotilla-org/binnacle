//! The ABI version: the header's `BINNACLE_ABI_VERSION` and the library's
//! `binnacle_abi_version()` agree, and the header's declarations cannot
//! change without the version being reconsidered.

use std::path::Path;

use binnacle_ffi::binnacle_abi_version;

/// The ABI version and the fingerprint of the header's declarations it was
/// pinned at. A change to the header outside its comments changes the
/// fingerprint: bump `BINNACLE_ABI_VERSION` (in the header and `lib.rs`) as
/// the header says, then pin the new pair here.
const PINNED: (u32, u64) = (19, 0xe2c0_d857_9729_d8c6);

fn header() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("include/binnacle.h");
    std::fs::read_to_string(path).expect("read binnacle.h")
}

fn header_version(header: &str) -> u32 {
    header
        .lines()
        .find_map(|line| line.strip_prefix("#define BINNACLE_ABI_VERSION "))
        .expect("binnacle.h defines BINNACLE_ABI_VERSION")
        .trim()
        .trim_end_matches('u')
        .parse()
        .expect("BINNACLE_ABI_VERSION is a number")
}

/// The header without its comments, whitespace runs folded to one space,
/// hashed with 64-bit FNV-1a (stable across Rust releases, unlike std's).
fn fingerprint(header: &str) -> u64 {
    let mut code = String::new();
    let mut rest = header;
    while let Some(start) = rest.find("/*") {
        code.push_str(&rest[..start]);
        code.push(' ');
        let end = rest[start + 2..].find("*/").expect("closed comment");
        rest = &rest[start + 2 + end + 2..];
    }
    code.push_str(rest);
    let folded = code.split_whitespace().collect::<Vec<_>>().join(" ");
    folded.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn the_library_reports_the_headers_abi_version() {
    assert_eq!(binnacle_abi_version(), header_version(&header()));
}

#[test]
fn the_headers_declarations_match_the_pinned_abi_version() {
    let header = header();
    let now = (header_version(&header), fingerprint(&header));
    assert_eq!(
        now, PINNED,
        "binnacle.h changed outside its comments. If a struct layout, a \
         constant's value or a function changed, or a declaration was added \
         or removed, bump BINNACLE_ABI_VERSION; then pin the new pair: \
         ({}, {:#x})",
        now.0, now.1
    );
}

#[test]
fn the_fingerprint_ignores_comments_and_layout_but_not_declarations() {
    let base = "/* a */\n#define X 1\nint f(int a);\n";
    assert_eq!(
        fingerprint(base),
        fingerprint("/* a\n * longer */ #define X 1\n\n int  f(int a); /* b */")
    );
    assert_ne!(fingerprint(base), fingerprint("#define X 2\nint f(int a);"));
    assert_ne!(
        fingerprint(base),
        fingerprint("#define X 1\nint f(long a);")
    );
}
