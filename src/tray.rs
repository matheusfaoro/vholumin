//! Ícone na bandeja do Windows — o único aviso visual do vholumin.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON, SM_CXSMICON, WM_APP};

use crate::icons::{self, IconState};

/// Mensagem que o Windows manda para a janela quando o ícone é clicado.
pub const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
    icons: Vec<HICON>,
    state: IconState,
    tip: String,
}

impl Tray {
    pub fn new(hwnd: HWND) -> Self {
        Self { hwnd, icons: load_icons(), state: IconState::Normal, tip: String::new() }
    }

    /// Coloca o ícone na bandeja. Devolve `false` se a barra de tarefas recusou.
    pub fn show(&mut self, state: IconState, tip: &str) -> bool {
        self.state = state;
        self.tip = tip.to_owned();
        let mut nid = self.data();
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        // SAFETY: estrutura completa e com cbSize correto.
        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool();
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        unsafe {
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }
        ok
    }

    /// Troca o ícone e o texto que aparece ao passar o mouse.
    pub fn set(&mut self, state: IconState, tip: &str) {
        self.state = state;
        self.tip = tip.to_owned();
        let mut nid = self.data();
        nid.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    /// O Explorer reiniciou: recria os ícones (a escala da tela pode ter mudado) e volta à bandeja.
    pub fn recreate(&mut self) {
        destroy_icons(&self.icons);
        self.icons = load_icons();
        let (state, tip) = (self.state, self.tip.clone());
        if !self.show(state, &tip) {
            logf!("não foi possível recolocar o ícone na bandeja");
        }
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icons[self.state as usize],
            ..Default::default()
        };
        let tip: Vec<u16> = self.tip.encode_utf16().take(nid.szTip.len() - 1).collect();
        nid.szTip[..tip.len()].copy_from_slice(&tip);
        nid
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let nid = self.data();
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        destroy_icons(&self.icons);
    }
}

fn load_icons() -> Vec<HICON> {
    // Tamanho de ícone pequeno na escala atual da tela (16 px a 100%, 24 px a 150%…).
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) }.max(16) as u32;
    icons::ALL
        .iter()
        .map(|&state| {
            icons::create_hicon(state, size).unwrap_or_else(|e| {
                logf!("falha ao criar o ícone {state:?}: {e}");
                HICON::default()
            })
        })
        .collect()
}

fn destroy_icons(icons: &[HICON]) {
    for &icon in icons {
        if !icon.is_invalid() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}
