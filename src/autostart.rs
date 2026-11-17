//! "Iniciar com o Windows": valor `vholumin` na chave Run do próprio usuário (HKCU).
//! Não precisa de administrador.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: PCWSTR = w!("vholumin");

/// Deixa o registro de acordo com `enabled`. Só escreve se algo mudou.
pub fn sync(enabled: bool) -> Result<(), String> {
    let current = read();
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\"", exe.display());
        if current.as_deref() == Some(command.as_str()) {
            return Ok(());
        }
        let wide: Vec<u16> = command.encode_utf16().chain([0]).collect();
        // SAFETY: o buffer tem o tamanho informado (em bytes, com o zero final).
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE_NAME,
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                (wide.len() * 2) as u32,
            )
            .ok()
            .map_err(|e| e.to_string())?;
        }
        logf!("iniciar com o Windows: ligado ({command})");
    } else if current.is_some() {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME).ok().map_err(|e| e.to_string())? };
        logf!("iniciar com o Windows: desligado");
    }
    Ok(())
}

/// O comando registrado hoje, se houver.
pub fn read() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: o tamanho informado é o do buffer, em bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}
