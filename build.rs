use std::path::Path;
fn main() {
    println!("cargo:rerun-if-changed=kernel/src/main.rs");
    let kernel = std::env::var("CARGO_BIN_FILE_KERNEL_kernel").expect("kernel binary not found");
    println!("cargo:warning=KERNEL = {}", kernel);
    let bios = bootloader::BiosBoot::new(Path::new(&kernel));

    bios.create_disk_image(Path::new("target/F_OS.img"))
        .unwrap();
}
