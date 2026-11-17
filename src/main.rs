//! vholumin — alterna o volume do jogo no Mixer do Windows com uma tecla de atalho.
//!
//! Regras anti-cheat (PLANO.md, seção 3): nunca abrir outro processo, nunca ler ou escrever
//! memória alheia, nunca instalar hook nem simular teclas. Só `RegisterHotKey`, a janela em
//! foco, o retrato da lista de processos e a Core Audio (a mesma API do Mixer do Windows).

#![cfg_attr(not(test), windows_subsystem = "windows")]

#[macro_use]
mod log;

mod app;
mod audio;
mod autostart;
mod config;
mod diag;
mod events;
mod hotkey;
mod icons;
mod paint;
mod processes;
mod settings;
mod target;
mod theme;
mod toggle;
mod tray;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None => app::run(),
        // Processo da janela de configurações, aberto pela cópia da bandeja.
        Some("--configuracoes") => settings::run(),
        Some("--listar") => diag::list(),
        Some("--alternar") => diag::toggle(args.get(1).map(String::as_str)),
        Some(other) => diag::usage_error(&format!("opção desconhecida: {other}")),
    };
    std::process::exit(code);
}
