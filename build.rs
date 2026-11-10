//! Compiles the save-actions helper (bridge/*.cs) with the .NET Framework C#
//! compiler that ships with Windows, against the original program's libraries.
//! When either is missing the app still builds; saving then reports that the
//! helper is unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let bridge = manifest.join("bridge");
    let sources = ["RdPonte.cs", "Azioni.cs", "Versamenti.cs"];
    for s in &sources {
        println!("cargo:rerun-if-changed=bridge/{s}");
    }
    println!("cargo:rerun-if-env-changed=RD_LIBDIR");

    let lib_dir = std::env::var("RD_LIBDIR").unwrap_or_else(|_| r"C:\Esprit\Raccolta Dati".to_string());
    let csc = Path::new(r"C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe");
    let out_dir = bridge.join("out");
    let exe = out_dir.join("RdPonte.exe");
    println!("cargo:rustc-env=RD_PONTE_DEFAULT={}", exe.display());

    if !csc.exists() || !Path::new(&lib_dir).join("RD.Logic.dll").exists() {
        println!("cargo:warning=save helper not built (C# compiler or original libraries missing)");
        return;
    }
    let _ = std::fs::create_dir_all(&out_dir);
    let mut cmd = Command::new(csc);
    cmd.current_dir(&bridge)
        .arg("/nologo")
        .arg("/platform:x86")
        .arg("/optimize")
        .arg(format!("/out:{}", exe.display()))
        .arg(format!("/lib:{lib_dir}"));
    for r in [
        "RD.Logic.dll",
        "RD.Dal.MSSql.dll",
        "RD.Entities.dll",
        "Esprit.WinForm.dll",
        "Esprit.Logic.dll",
        "Esprit.Entities.dll",
        "Esprit.Sys.Dal.dll",
        "Microsoft.Data.SqlClient.dll",
        "netstandard.dll",
        "System.Data.dll",
    ] {
        cmd.arg(format!("/r:{r}"));
    }
    cmd.args(sources);
    match cmd.output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            for line in text.lines().take(10) {
                println!("cargo:warning=save helper: {line}");
            }
        }
        Err(e) => println!("cargo:warning=save helper not built: {e}"),
    }
}
