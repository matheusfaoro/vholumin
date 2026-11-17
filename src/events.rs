//! Avisos do Windows sobre as sessões de áudio que o vholumin acabou de alternar: volume
//! mudado por fora (pelo Mixer, por exemplo) ou jogo fechado. Assim o ícone se corrige sozinho.
//!
//! Os avisos chegam numa thread do sistema de áudio e são repassados para a janela do
//! vholumin com `PostMessageW`. Nada é verificado periodicamente.

use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Media::Audio::{
    AudioSessionDisconnectReason, AudioSessionState, AudioSessionStateExpired, IAudioSessionControl,
    IAudioSessionEvents, IAudioSessionEvents_Impl,
};
use windows::Win32::System::Com::CoCreateGuid;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{BOOL, GUID, PCWSTR, Result, implement};

/// Mensagem com um aviso de áudio (`wparam` = geração << 4 | tipo).
pub const WM_AUDIO_EVENT: u32 = WM_APP + 2;
/// O volume mudou por fora; `lparam` = bits do novo volume (f32).
pub const EVENT_VOLUME: usize = 1;
/// A sessão acabou (jogo fechado, dispositivo removido); `lparam` = índice da sessão.
pub const EVENT_GONE: usize = 2;

/// Identifica as mudanças de volume feitas por este processo, para ignorar o próprio eco.
pub fn own_context() -> &'static GUID {
    static CONTEXT: OnceLock<GUID> = OnceLock::new();
    CONTEXT.get_or_init(|| unsafe { CoCreateGuid() }.unwrap_or(GUID::from_u128(0x6f1c_2a4e_9b7d_4c3a_a8e2_5d0b_7c9f_1e42)))
}

/// Separa geração e tipo de um aviso.
pub fn decode(wparam: WPARAM) -> (usize, usize) {
    (wparam.0 >> 4, wparam.0 & 0xF)
}

#[implement(IAudioSessionEvents)]
struct SessionEvents {
    hwnd: isize,
    generation: usize,
    index: usize,
}

impl SessionEvents {
    fn post(&self, kind: usize, data: isize) {
        let hwnd = HWND(self.hwnd as *mut _);
        // SAFETY: PostMessageW pode ser chamada de qualquer thread.
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_AUDIO_EVENT, WPARAM(self.generation << 4 | kind), LPARAM(data));
        }
    }
}

impl IAudioSessionEvents_Impl for SessionEvents_Impl {
    fn OnDisplayNameChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnIconPathChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnSimpleVolumeChanged(&self, volume: f32, _mute: BOOL, context: *const GUID) -> Result<()> {
        // SAFETY: o Windows passa um ponteiro válido ou nulo.
        let own = !context.is_null() && unsafe { *context } == *own_context();
        if !own {
            self.post(EVENT_VOLUME, volume.to_bits() as isize);
        }
        Ok(())
    }

    fn OnChannelVolumeChanged(&self, _: u32, _: *const f32, _: u32, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnGroupingParamChanged(&self, _: *const GUID, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnStateChanged(&self, state: AudioSessionState) -> Result<()> {
        if state == AudioSessionStateExpired {
            self.post(EVENT_GONE, self.index as isize);
        }
        Ok(())
    }

    fn OnSessionDisconnected(&self, _: AudioSessionDisconnectReason) -> Result<()> {
        self.post(EVENT_GONE, self.index as isize);
        Ok(())
    }
}

/// As sessões sendo acompanhadas depois da última alternância.
pub struct Watch {
    pub generation: usize,
    /// Nomes dos programas, para o texto do ícone.
    pub names: String,
    items: Vec<(IAudioSessionControl, IAudioSessionEvents)>,
    alive: Vec<bool>,
}

impl Watch {
    pub fn new<'a>(
        hwnd: HWND,
        generation: usize,
        names: String,
        controls: impl Iterator<Item = &'a IAudioSessionControl>,
    ) -> Self {
        let mut items = Vec::new();
        for control in controls {
            let events: IAudioSessionEvents =
                SessionEvents { hwnd: hwnd.0 as isize, generation, index: items.len() }.into();
            // SAFETY: chamada COM simples; a inscrição é desfeita no Drop.
            match unsafe { control.RegisterAudioSessionNotification(&events) } {
                Ok(()) => items.push((control.clone(), events)),
                Err(e) => logf!("não foi possível acompanhar uma sessão de áudio: {e}"),
            }
        }
        let alive = vec![true; items.len()];
        Self { generation, names, items, alive }
    }

    /// Marca a sessão `index` como encerrada. Devolve `true` se não sobrou nenhuma.
    pub fn mark_gone(&mut self, index: usize) -> bool {
        if let Some(alive) = self.alive.get_mut(index) {
            *alive = false;
        }
        !self.alive.iter().any(|&a| a)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        for (control, events) in &self.items {
            unsafe {
                let _ = control.UnregisterAudioSessionNotification(events);
            }
        }
    }
}
