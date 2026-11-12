use std::path::{Path, PathBuf};
use std::process::Command;

fn describe(file: &str, description: &str, icon: Option<&Path>) {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../winres.rs");
    println!("cargo:rerun-if-env-changed=PATH");
    if let Some(icon) = icon {
        println!("cargo:rerun-if-changed={}", icon.display());
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets the version");
    let mut parts = version.split(['.', '-', '+']).map(|part| part.parse::<u16>().unwrap_or(0));
    let mut number = || parts.next().unwrap_or(0);
    let numbers = format!("{},{},{},0", number(), number(), number());

    let icon = icon.map(|icon| format!("1 ICON \"{}\"\n", icon.display().to_string().replace('\\', "/"))).unwrap_or_default();
    let script = format!(
        r#"#pragma code_page(65001)
{icon}1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "Fluxen"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{file}"
      VALUE "LegalCopyright", "Copyright (C) 2026 Fluxen. GNU General Public License, version 3 or later."
      VALUE "OriginalFilename", "{file}"
      VALUE "ProductName", "Dota 2 Mod Enjoyer"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let (source, compiled) = (out.join("about.rc"), out.join("about.o"));
    std::fs::write(&source, script).expect("the build folder can be written to");
    let done = Command::new("windres").arg("--input").arg(&source).arg("--output").arg(&compiled).arg("--output-format=coff").status();
    match done {
        Ok(status) if status.success() => println!("cargo:rustc-link-arg-bins={}", compiled.display()),
        Ok(status) => println!("cargo:warning=windres failed ({status}): {file} is built without its details and its icon"),
        Err(_) => println!("cargo:warning=windres was not found: {file} is built without its details and its icon"),
    }
}
