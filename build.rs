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

    // Standard Windows RC format with icon and VERSIONINFO
    let ico_path_str = ico_path.to_str().unwrap().replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "1.8.0".to_string());
    let mut version_parts = version.split('.');
    let major: u16 = version_parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
    let minor: u16 = version_parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let patch: u16 = version_parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);

    let rc_content = format!(
        r#"MAINICON ICON "{ico_path_str}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "LycosLane"
            VALUE "FileDescription", "FreeDataDiet - Disk Usage Analyzer"
            VALUE "FileVersion", "{version}.0"
            VALUE "InternalName", "freedatadiet"
            VALUE "LegalCopyright", "Copyright (c) 2026 LycosLane"
            VALUE "OriginalFilename", "freedatadiet.exe"
            VALUE "ProductName", "FreeDataDiet"
            VALUE "ProductVersion", "{version}.0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );
    if fs::write(&rc_path, rc_content).is_err() {
        return;
    }

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_env == "msvc" {
        let res_path = PathBuf::from(&out_dir).join("freedatadiet.res");
        let status = Command::new("rc.exe")
            .args(["/fo", res_path.to_str().unwrap(), rc_path.to_str().unwrap()])
            .status();

        if let Ok(s) = status
            && s.success() {
                println!("cargo:rustc-link-arg-bins={}", res_path.to_str().unwrap());
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

        if let Ok(s) = status
            && s.success() {
                // Do not link Windows resource object into test runner binaries
                println!("cargo:rustc-link-arg-bins={}", o_path.to_str().unwrap());
            }
    }
}
