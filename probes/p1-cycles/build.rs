fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rustc-link-search={}", root.display());
    println!("cargo:rerun-if-changed=memory.x");
}
