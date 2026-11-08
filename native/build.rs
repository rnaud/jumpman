//! Compiles the GameController.framework shim (Apple targets only).
fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os == "macos" || os == "ios" {
        println!("cargo:rerun-if-changed=src/controller/gc_shim.m");
        cc::Build::new().file("src/controller/gc_shim.m").flag("-fobjc-arc").flag("-fmodules").warnings(false).compile("jm_gc_shim");
        println!("cargo:rustc-link-lib=framework=GameController");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
