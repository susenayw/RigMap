use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
        let res_obj = out_dir.join("app_icon_res.o");

        let status = Command::new("windres")
            .args([
                "assets/app.rc",
                "-O",
                "coff",
                "-o",
                &res_obj.to_string_lossy(),
            ])
            .status();

        if let Ok(st) = status {
            if st.success() {
                println!("cargo:rustc-link-arg-bins={}", res_obj.display());
            }
        }
    }
}
