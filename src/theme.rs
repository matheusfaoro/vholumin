//! Tema escuro do que não é desenhado por nós: a barra de título da janela de configurações
//! e os menus (o da bandeja e o de "Adicionar tocando agora").

use std::ffi::c_void;

use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{BOOL, PCSTR, w};

/// Barra de título escura (Windows 10 1809 em diante; antes disso, fica clara).
pub fn dark_title_bar(hwnd: HWND) {
    let on = BOOL(1);
    let value = &on as *const BOOL as *const c_void;
    unsafe {
        // DWMWA_USE_IMMERSIVE_DARK_MODE é 20 no Windows 10 20H1+ e no 11, e 19 antes disso.
        if DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(20), value, 4).is_err() {
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(19), value, 4);
        }
    }
}

/// Menus escuros no processo inteiro. Usa funções do Windows que não são documentadas
/// (as mesmas que Notepad++ e outros usam), só no Windows 10 1903+ onde elas existem com
/// esse formato. Se algo não estiver lá, os menus continuam claros — nada quebra.
pub fn enable_dark_menus() {
    if windows_build() < 18362 {
        return;
    }
    unsafe {
        let Ok(uxtheme) = LoadLibraryExW(w!("uxtheme.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) else { return };
        // Ordinal 135 = SetPreferredAppMode(modo); 2 = sempre escuro.
        if let Some(set_preferred_app_mode) = GetProcAddress(uxtheme, PCSTR(135 as *const u8)) {
            let set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32 =
                std::mem::transmute(set_preferred_app_mode);
            set_preferred_app_mode(2);
        }
        // Ordinal 136 = FlushMenuThemes().
        if let Some(flush_menu_themes) = GetProcAddress(uxtheme, PCSTR(136 as *const u8)) {
            let flush_menu_themes: unsafe extern "system" fn() = std::mem::transmute(flush_menu_themes);
            flush_menu_themes();
        }
    }
}

/// Número do build do Windows (ex.: 19045), ou 0 se não der para ler.
fn windows_build() -> u32 {
    let mut buffer = [0u16; 32];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return 0;
    }
    let len = (size as usize / 2).saturating_sub(1);
    String::from_utf16_lossy(&buffer[..len]).trim().parse().unwrap_or(0)
}
