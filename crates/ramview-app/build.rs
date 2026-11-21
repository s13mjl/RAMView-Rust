fn main() {
    println!("cargo:rerun-if-changed=ramview.rc");
    println!("cargo:rerun-if-changed=../../assets/ramview.ico");

    #[cfg(windows)]
    embed_resource::compile("ramview.rc", embed_resource::NONE);
}
