//! Sessões de áudio do Windows (o que o Mixer mostra), via Core Audio.
//!
//! Tudo aqui conversa com o serviço de áudio do Windows; nenhum processo é aberto.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Audio::{
    AudioSessionState, AudioSessionStateActive, AudioSessionStateExpired, AudioSessionStateInactive,
    DEVICE_STATE_ACTIVE, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::{Interface, Result};

/// Uma sessão de áudio de um programa num dispositivo de saída.
pub struct Session {
    pub pid: u32,
    pub device: String,
    pub state: AudioSessionState,
    /// Volume no momento da leitura (0.0 a 1.0).
    pub volume: f32,
    control: IAudioSessionControl,
    volume_control: ISimpleAudioVolume,
}

impl Session {
    /// Muda o volume, marcando a mudança como feita por este processo (ver `events`).
    pub fn set_volume(&self, level: f32) -> Result<()> {
        // SAFETY: chamada COM simples numa interface válida.
        unsafe { self.volume_control.SetMasterVolume(level.clamp(0.0, 1.0), crate::events::own_context()) }
    }

    /// A interface usada para acompanhar os avisos da sessão.
    pub fn control(&self) -> &IAudioSessionControl {
        &self.control
    }

    pub fn state_name(&self) -> &'static str {
        if self.state == AudioSessionStateActive {
            "tocando"
        } else if self.state == AudioSessionStateInactive {
            "parado"
        } else {
            "encerrado"
        }
    }
}

/// Lista as sessões de áudio de todos os dispositivos de saída ativos (fone, caixa, HDMI…),
/// sem a sessão "Sons do sistema". Precisa do COM inicializado na thread.
pub fn list_sessions() -> Result<Vec<Session>> {
    let mut sessions = Vec::new();
    // SAFETY: chamadas COM com o COM já inicializado pelo chamador.
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for i in 0..devices.GetCount()? {
            let Ok(device) = devices.Item(i) else { continue };
            let name = device_name(&device);
            if let Err(e) = collect_device_sessions(&device, &name, &mut sessions) {
                logf!("dispositivo '{name}' ignorado: {e}");
            }
        }
    }
    Ok(sessions)
}

unsafe fn collect_device_sessions(device: &IMMDevice, name: &str, out: &mut Vec<Session>) -> Result<()> {
    unsafe {
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let list = manager.GetSessionEnumerator()?;
        for i in 0..list.GetCount()? {
            let Ok(control) = list.GetSession(i) else { continue };
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
            if control2.IsSystemSoundsSession() == S_OK {
                continue;
            }
            let state = control.GetState().unwrap_or(AudioSessionStateInactive);
            if state == AudioSessionStateExpired {
                continue;
            }
            let Ok(pid) = control2.GetProcessId() else { continue };
            let Ok(volume_control) = control.cast::<ISimpleAudioVolume>() else { continue };
            let volume = volume_control.GetMasterVolume().unwrap_or(1.0);
            out.push(Session { pid, device: name.to_owned(), state, volume, control, volume_control });
        }
    }
    Ok(())
}

/// Nome amigável do dispositivo ("Fones de ouvido (Realtek)"), ou "?".
unsafe fn device_name(device: &IMMDevice) -> String {
    unsafe {
        let Ok(store) = device.OpenPropertyStore(STGM_READ) else { return "?".into() };
        let Ok(mut value) = store.GetValue(&PKEY_Device_FriendlyName) else { return "?".into() };
        let name = match PropVariantToStringAlloc(&value) {
            Ok(text) => {
                let s = text.to_string().unwrap_or_else(|_| "?".into());
                CoTaskMemFree(Some(text.0 as _));
                s
            }
            Err(_) => "?".into(),
        };
        let _ = PropVariantClear(&mut value);
        name
    }
}
