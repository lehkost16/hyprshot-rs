use std::process::Command;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/longshot/overlay.c");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("shot-overlay");
    
    let status = Command::new("gcc")
        .args(&[
            "-O3",
            "src/longshot/overlay.c",
            "-o",
            dest_path.to_str().unwrap(),
            "-lraylib",
            "-lGL",
            "-lm",
            "-lpthread",
            "-ldl",
            "-lrt",
        ])
        .status()
        .expect("Failed to compile C overlay");
    if !status.success() {
        panic!("C overlay compilation failed");
    }
    
    // Copy the compiled binary next to the main shot binary
    if let Ok(_profile) = std::env::var("PROFILE") {
        let target_dir = Path::new(&out_dir)
            .ancestors()
            .nth(3) // target/debug or target/release
            .unwrap()
            .join("shot-overlay");
        let _ = std::fs::copy(&dest_path, &target_dir);
    }
}
