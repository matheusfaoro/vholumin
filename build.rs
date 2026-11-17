//! Embute no .exe o manifesto (visual moderno, escala por monitor, sem pedir administrador)
//! e a janela de configurações, descritos em `res/vholumin.rc`.

fn main() {
    println!("cargo:rerun-if-changed=res/vholumin.rc");
    println!("cargo:rerun-if-changed=res/vholumin.manifest");
    embed_resource::compile("res/vholumin.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
