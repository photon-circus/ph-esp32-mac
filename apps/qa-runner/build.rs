//! Build script for the QA runner
//!
//! This primarily provides helpful error messages if building on the wrong target.

fn main() {
    // Only compile for Xtensa ESP32
    let target = std::env::var("TARGET").unwrap_or_default();

    if !target.contains("xtensa-esp32") {
        println!("cargo:warning==============================================");
        println!("cargo:warning= Building for target: {}", target);
        println!("cargo:warning= This crate requires: xtensa-esp32-none-elf");
        println!("cargo:warning=");
        println!("cargo:warning= Install the ESP32 toolchain:");
        println!("cargo:warning=   cargo install espup");
        println!("cargo:warning=   espup install");
        println!("cargo:warning=");
        println!("cargo:warning= Then build with:");
        println!("cargo:warning=   cargo xtask build qa-runner");
        println!("cargo:warning= (or set --target xtensa-esp32-none-elf -Zbuild-std=core manually)");
        println!("cargo:warning==============================================");
    }

    // Rerun if target changes
    println!("cargo:rerun-if-env-changed=TARGET");
}
