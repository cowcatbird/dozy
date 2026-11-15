fn main() {
    println!("cargo:rerun-if-changed=src/macos/shim.m");
    cc::Build::new().file("src/macos/shim.m").flag("-fobjc-arc").flag("-Wno-unused-parameter").compile("dozy_shim");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
}
