//! Runs in its own process, as the PCAN library is loaded once per process.
#![cfg(all(feature = "pcan", target_os = "linux", target_arch = "x86_64"))]

use async_can::pcan;

/// Loading the bundled PCAN library must not leave the extracted copy behind.
#[tokio::test]
async fn library_temp_file_is_removed() {
    let dir = std::env::temp_dir().join(format!("async-can-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: this is the only test in this binary, and it runs before any other thread reads
    // the environment.
    unsafe { std::env::set_var("TMPDIR", &dir) };

    // Loads the library. The call itself fails if no PCAN driver is installed, which is fine.
    let _ = pcan::list_devices().await;

    let leftover: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(leftover.is_empty(), "left behind: {leftover:?}");
}
