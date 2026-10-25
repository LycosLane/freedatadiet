use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/freedatadiet.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let ico_path = Path::new(&manifest_dir).join("assets").join("icons").join("freedatadiet.ico");

    if !ico_path.exists() {
        return;
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let rc_path = PathBuf::from(&out_dir).join("freedatadiet.rc");

    // Standard Windows RC format
    let ico_path_str = ico_path.to_str().unwrap().replace('\\', "/");
    let rc_content = format!("MAINICON ICON \"{}\"\n", ico_path_str);
    if fs::write(&rc_path, rc_content).is_err() {
        return;
    }

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_env == "msvc" {
        let res_path = PathBuf::from(&out_dir).join("freedatadiet.res");
        let status = Command::new("rc.exe")
            .args(["/fo", res_path.to_str().unwrap(), rc_path.to_str().unwrap()])
            .status();

        if let Ok(s) = status {
            if s.success() {
                println!("cargo:rustc-link-arg={}", res_path.to_str().unwrap());
            }
        }
    } else {
        // GNU / MinGW toolchains (windres / llvm-windres)
        let o_path = PathBuf::from(&out_dir).join("freedatadiet_res.o");
        let status = Command::new("windres")
            .args(["-i", rc_path.to_str().unwrap(), "-o", o_path.to_str().unwrap()])
            .status()
            .or_else(|_| {
                Command::new("llvm-windres")
                    .args(["-i", rc_path.to_str().unwrap(), "-o", o_path.to_str().unwrap()])
                    .status()
            });

        if let Ok(s) = status {
            if s.success() {
                println!("cargo:rustc-link-arg={}", o_path.to_str().unwrap());
            }
        }
    }
}
