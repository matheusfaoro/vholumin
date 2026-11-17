//! Recursos embutidos no .exe: o manifesto (visual atual dos controles, escala por monitor,
//! sem pedir administrador), o ícone do app e as informações de versão — que vêm do
//! Cargo.toml, então a versão só precisa ser trocada num lugar.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=res/vholumin.manifest");
    println!("cargo:rerun-if-changed=res/vholumin.ico");

    // Barras normais: o compilador de recursos aceita, e não há escape de "\" para errar.
    let root = env::var("CARGO_MANIFEST_DIR").unwrap().replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let number = |name: &str| env::var(name).unwrap();
    let (major, minor, patch) =
        (number("CARGO_PKG_VERSION_MAJOR"), number("CARGO_PKG_VERSION_MINOR"), number("CARGO_PKG_VERSION_PATCH"));

    let rc = format!(
        r#"#pragma code_page(65001)
#include <windows.h>

CREATEPROCESS_MANIFEST_RESOURCE_ID RT_MANIFEST "{root}/res/vholumin.manifest"
1 ICON "{root}/res/vholumin.ico"

VS_VERSION_INFO VERSIONINFO
 FILEVERSION {major},{minor},{patch},0
 PRODUCTVERSION {major},{minor},{patch},0
 FILEFLAGSMASK VS_FFI_FILEFLAGSMASK
 FILEFLAGS 0
 FILEOS VOS_NT_WINDOWS32
 FILETYPE VFT_APP
 FILESUBTYPE VFT2_UNKNOWN
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "041604B0"
        BEGIN
            VALUE "CompanyName", "Matheus Faoro"
            VALUE "FileDescription", "vholumin"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "vholumin"
            VALUE "LegalCopyright", "© Matheus Faoro"
            VALUE "OriginalFilename", "vholumin.exe"
            VALUE "ProductName", "vholumin"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0416, 1200
    END
END
"#
    );
    let rc_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("vholumin.rc");
    fs::write(&rc_path, rc).unwrap();
    embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional().unwrap();
}
