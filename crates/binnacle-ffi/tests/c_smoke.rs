//! Builds `tests/smoke.c` against `include/binnacle.h` and the static library
//! cargo built for this test run, then runs it.

use std::path::Path;
use std::process::Command;

/// What the Rust standard library needs from the system when linked
/// statically (`--print native-static-libs`).
const NATIVE_LIBS: &[&str] = if cfg!(target_os = "macos") {
    &["-lSystem", "-lc", "-lm"]
} else {
    &[
        "-lgcc_s",
        "-lutil",
        "-lrt",
        "-lpthread",
        "-lm",
        "-ldl",
        "-lc",
    ]
};

#[cfg(unix)]
#[test]
fn the_c_smoke_program_round_trips_a_frame() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = std::env::current_exe().unwrap();
    let staticlib = exe.with_file_name("libbinnacle_ffi.a");
    assert!(
        staticlib.exists(),
        "no static library at {}",
        staticlib.display()
    );
    let program = Path::new(env!("CARGO_TARGET_TMPDIR")).join("binnacle-smoke");

    let built = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(crate_dir.join("include"))
        .arg(crate_dir.join("tests/smoke.c"))
        .arg(&staticlib)
        .args(NATIVE_LIBS)
        .arg("-o")
        .arg(&program)
        .output()
        .expect("run cc");
    assert!(
        built.status.success(),
        "cc failed:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let ran = Command::new(&program)
        .output()
        .expect("run the smoke program");
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert!(
        ran.status.success(),
        "smoke program failed ({}):\n{stdout}{}",
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
    assert!(stdout.ends_with("ok\n"), "unexpected output:\n{stdout}");
}
