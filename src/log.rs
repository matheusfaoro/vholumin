//! Log simples em `%APPDATA%\vholumin\log.txt`, com rotação em ~1 MB.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use windows::Win32::System::SystemInformation::GetLocalTime;

const MAX_BYTES: u64 = 1024 * 1024;

/// Escreve uma linha no log: `logf!("texto {}", valor)`.
macro_rules! logf {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}

/// Pasta de dados do vholumin (`%APPDATA%\vholumin`). A variável `VHOLUMIN_DATA_DIR` troca a
/// pasta — usada nos testes para não mexer nas configurações de verdade.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("VHOLUMIN_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("vholumin"))
}

pub fn write(message: &str) {
    let Some(dir) = data_dir() else { return };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("log.txt");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&path, dir.join("log.old.txt"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        // Uma única escrita por linha: duas cópias escrevendo juntas não misturam as linhas.
        let line = format!("{} {message}\n", timestamp());
        let _ = file.write_all(line.as_bytes());
    }
}

fn timestamp() -> String {
    // SAFETY: GetLocalTime só preenche e devolve uma estrutura.
    let t = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}
