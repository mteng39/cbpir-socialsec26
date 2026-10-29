use std::env;
use std::path::PathBuf;

fn main() {
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    let prefix = PathBuf::from(
        env::var_os("OPENFHE_INSTALL_DIR")
            .expect("set OPENFHE_INSTALL_DIR to the exact OpenFHE 1.5.1 install prefix"),
    );
    let native_lib_dir = prefix
        .join("lib")
        .canonicalize()
        .expect("OPENFHE_INSTALL_DIR must contain a readable lib directory");

    println!("cargo::rerun-if-env-changed=OPENFHE_INSTALL_DIR");
    println!("cargo::rustc-link-arg=-Wl,--disable-new-dtags");
    println!(
        "cargo::rustc-link-arg=-Wl,-rpath,{}",
        native_lib_dir.display()
    );
}
