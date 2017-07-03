
fn main () {
    if !cfg!(feature = "rust-mount") {
        println!("cargo:rustc-link-lib=fuse");
    }
}
