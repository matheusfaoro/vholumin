//! Comandos de diagnóstico pela linha de comando:
//!
//! - `vholumin --listar` — mostra todas as sessões de áudio (como o Mixer vê).
//! - `vholumin --alternar <PID>` — faz o que a bind faria se o processo `<PID>` estivesse em
//!   foco, com os volumes do config.toml.

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
};
use windows::core::w;

use crate::audio;
use crate::config::Config;
use crate::processes::ProcessTable;
use crate::toggle::{self, Outcome, Target, pct};

const USAGE: &str = "uso: vholumin [--listar | --alternar <PID>]";

pub fn list() -> i32 {
    attach_console();
    let result = with_com(|| {
        let procs = ProcessTable::snapshot()?;
        let sessions = audio::list_sessions()?;
        println!("{:>7}  {:<34} {:>6}  {:<9}  Dispositivo", "PID", "Programa", "Volume", "Estado");
        for s in &sessions {
            let exe = procs.exe(s.pid).unwrap_or("?");
            println!("{:>7}  {:<34} {:>5}%  {:<9}  {}", s.pid, exe, pct(s.volume), s.state_name(), s.device);
        }
        if sessions.is_empty() {
            println!("(nenhuma sessão de áudio)");
        }
        Ok(0)
    });
    result.unwrap_or_else(|e| {
        eprintln!("erro: {e}");
        1
    })
}

pub fn toggle(arg: Option<&str>) -> i32 {
    attach_console();
    let Some(pid) = arg.and_then(|a| a.parse::<u32>().ok()) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let range = Config::load().config.range();
    let result = with_com(|| {
        // O relatório (com objetos COM) é usado e solto aqui dentro, antes de fechar o COM.
        Ok(match toggle::toggle(Target::Focus(pid), range)? {
            Outcome::Changed(r) => {
                println!("alvo: {} (encontrado por {:?}) → {}%", r.target_names(), r.kind, pct(r.level));
                for c in &r.changes {
                    let status = if c.ok { "" } else { " (FALHOU)" };
                    println!(
                        "  {} (PID {}) em '{}': {}% → {}%{status}",
                        c.exe,
                        c.session.pid,
                        c.session.device,
                        pct(c.before),
                        pct(r.level)
                    );
                }
                0
            }
            Outcome::NotFound(what) => {
                println!("{what} (PID {pid})");
                3
            }
        })
    });
    result.unwrap_or_else(|e| {
        eprintln!("erro: {e}");
        1
    })
}

pub fn usage_error(message: &str) -> i32 {
    attach_console();
    eprintln!("{message}\n{USAGE}");
    2
}

fn with_com<T>(f: impl FnOnce() -> windows::core::Result<T>) -> windows::core::Result<T> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let result = f();
    unsafe { CoUninitialize() };
    result
}

/// O vholumin é um programa de janela (sem console). Para os comandos de diagnóstico,
/// usa a saída já redirecionada, se houver, ou se pendura no console de quem chamou.
fn attach_console() {
    let valid = |std| unsafe { GetStdHandle(std) }.is_ok_and(|h| !h.is_invalid());
    if valid(STD_OUTPUT_HANDLE) {
        return;
    }
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            return;
        }
        for std in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            if valid(std) {
                continue;
            }
            if let Ok(handle) = CreateFileW(
                w!("CONOUT$"),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            ) {
                let _ = SetStdHandle(std, handle);
            }
        }
    }
}
