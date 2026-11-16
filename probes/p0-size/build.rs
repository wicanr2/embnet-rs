// 讓 cortex-m-rt 的 link.x 找到 workspace 根目錄的 memory.x。
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    println!("cargo:rustc-link-search={}", root.canonicalize().unwrap().display());
    println!("cargo:rerun-if-changed=../memory.x");
}
