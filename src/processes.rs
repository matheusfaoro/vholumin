//! Retrato da lista de processos do sistema: PID e nome do `.exe`.
//!
//! Usa `CreateToolhelp32Snapshot`, que lê a lista do sistema sem abrir nenhum processo.

use std::collections::HashMap;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

#[derive(Debug, Default)]
pub struct ProcessTable {
    exes: HashMap<u32, String>,
}

impl ProcessTable {
    /// Tira o retrato atual da lista de processos.
    pub fn snapshot() -> windows::core::Result<Self> {
        let mut table = Self::default();
        // SAFETY: o handle do retrato é fechado no fim; a estrutura tem o dwSize correto.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut ok = Process32FirstW(snap, &mut entry).is_ok();
            while ok {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                table.insert(entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..len]));
                ok = Process32NextW(snap, &mut entry).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        Ok(table)
    }

    pub fn insert(&mut self, pid: u32, exe: impl Into<String>) {
        self.exes.insert(pid, exe.into());
    }

    /// Nome do `.exe` do processo, se ele existe no retrato.
    pub fn exe(&self, pid: u32) -> Option<&str> {
        self.exes.get(&pid).map(String::as_str)
    }
}
