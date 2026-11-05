//! Runs in its own process, as the PCAN library is loaded once per process.
#![cfg(feature = "pcan")]

use async_can::{Error, pcan};

/// If the PCAN library cannot be loaded, the API must return an error rather than panic.
#[tokio::test]
async fn library_load_failure_returns_error() {
    // Make extracting the bundled library fail by pointing the temp directory below a regular
    // file, where no directory or file can be created.
    let file = std::env::temp_dir().join(format!("async-can-test-{}", std::process::id()));
    std::fs::write(&file, b"").unwrap();
    let temp_dir = file.join("temp");
    // SAFETY: this is the only test in this binary, and it runs before any other thread reads
    // the environment.
    unsafe {
        for var in ["TMPDIR", "TMP", "TEMP"] {
            std::env::set_var(var, &temp_dir);
        }
    }
    assert!(matches!(
        pcan::list_devices().await,
        Err(Error::PCanLibraryLoad(_))
    ));
    assert!(matches!(
        pcan::Sender::connect("usb1", 125000),
        Err(Error::PCanLibraryLoad(_))
    ));
    std::fs::remove_file(&file).unwrap();
}
