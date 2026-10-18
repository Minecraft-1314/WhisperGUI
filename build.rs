use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .and_then(|text| {
            text.split_whitespace()
                .nth(1)
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=APP_RUST_VERSION={version}");

    embed_icon();
}

fn embed_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let icon = PathBuf::from(manifest_dir).join("app.ico");
    if !icon.is_file() {
        println!("cargo:warning=app.ico not found; the executable icon resource is skipped");
        return;
    }
    println!("cargo:rerun-if-changed=app.ico");

    let package = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| String::from("app"));
    let staging = std::env::temp_dir().join(format!("rust-app-icon-{package}"));
    if std::fs::create_dir_all(&staging).is_err() {
        println!("cargo:warning=unable to create the icon staging directory");
        return;
    }

    let staged_icon = staging.join("app.ico");
    let resource = staging.join("app.rc");
    let object = staging.join("app.o");
    if std::fs::copy(&icon, &staged_icon).is_err() {
        println!("cargo:warning=unable to stage app.ico for the resource compiler");
        return;
    }
    if std::fs::write(&resource, "1 ICON \"app.ico\"\n").is_err() {
        println!("cargo:warning=unable to write the icon resource script");
        return;
    }
    let _ = std::fs::remove_file(&object);

    let compiled = Command::new("windres")
        .current_dir(&staging)
        .arg("-i")
        .arg(&resource)
        .arg("-o")
        .arg(&object)
        .arg("-O")
        .arg("coff")
        .status();

    match compiled {
        Ok(status) if status.success() && object.is_file() => {
            println!("cargo:rustc-link-arg={}", object.display());
        }
        _ => println!(
            "cargo:warning=windres is unavailable; the executable icon resource is skipped"
        ),
    }
}
