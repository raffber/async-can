use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("CARGO_FEATURE_PCAN").is_ok() {
        println!("cargo:rerun-if-changed=include/wrapper.h");

        let bindings = bindgen::Builder::default()
            .header("include/wrapper.h")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .generate()
            .expect("Unable to generate bindings");

        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
