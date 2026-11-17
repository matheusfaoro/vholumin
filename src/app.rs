//! O programa em si: janela invisível, bind, ícone na bandeja e o laço de mensagens.
//!
//! A thread principal fica parada em `GetMessageW` e só acorda com a bind, um clique no
//! ícone, um aviso de áudio da sessão alternada ou o fim da piscada de "nada encontrado".
//! Nada roda periodicamente.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::Shell::{NIN_SELECT, NINF_KEY, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    FindWindowW, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HMENU, KillTimer,
    MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SW_SHOWNORMAL, SetForegroundWindow, SetTimer, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU,
    WM_DESTROY, WM_HOTKEY, WM_NULL, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::autostart;
use crate::config::{Config, Loaded, Mode};
use crate::events::{self, EVENT_GONE, EVENT_VOLUME, WM_AUDIO_EVENT, Watch};
use crate::icons::IconState;
use crate::target::{TOLERANCE, Toggle};
use crate::toggle::{self, Outcome, Target, pct};
use crate::tray::{Tray, WM_TRAY};

const HOTKEY_ID: i32 = 1;
const FLASH_TIMER: usize = 1;
const FLASH_MS: u32 = 1500;
const WINDOW_CLASS: PCWSTR = w!("vholumin.janela");
/// Nome do mutex de instância única (o instalador da Fase 4 usa o mesmo nome).
const INSTANCE_MUTEX: PCWSTR = w!("vholumin.instancia");
/// Pedido de uma segunda cópia do vholumin para abrir as configurações.
const WM_OPEN_SETTINGS: u32 = WM_APP + 3;

const CMD_MODE_FOCUS: i32 = 10;
const CMD_MODE_LIST: i32 = 11;
const CMD_PAUSE: i32 = 12;
const CMD_AUTOSTART: i32 = 13;
const CMD_EDIT_CONFIG: i32 = 14;
const CMD_RELOAD: i32 = 15;
const CMD_OPEN_LOGS: i32 = 16;
const CMD_EXIT: i32 = 17;

/// Mensagem que o Explorer manda a todos quando a barra de tarefas é recriada.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeyState {
    Registered,
    Paused,
    /// Já usada por outro programa.
    Taken,
    Failed,
}

struct App {
    hwnd: HWND,
    tray: Tray,
    config: Config,
    problems: Vec<String>,
    hotkey: HotkeyState,
    /// Ícone e texto "de verdade" (para onde volta depois de uma piscada).
    steady: (IconState, String),
    /// Sessões alternadas por último, acompanhadas pelos avisos de áudio.
    watch: Option<Watch>,
    generation: usize,
}

pub fn run() -> i32 {
    if !acquire_instance() {
        // Já tem um vholumin rodando: pede para ele abrir as configurações e sai.
        unsafe {
            if let Ok(hwnd) = FindWindowW(WINDOW_CLASS, PCWSTR::null()) {
                let _ = PostMessageW(Some(hwnd), WM_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
            }
        }
        return 0;
    }
    match run_inner() {
        Ok(()) => 0,
        Err(e) => {
            logf!("erro fatal: {e}");
            1
        }
    }
}

/// `true` se esta é a única cópia rodando. O mutex fica aberto até o processo terminar.
fn acquire_instance() -> bool {
    // SAFETY: o GetLastError é lido logo depois do CreateMutexW.
    unsafe {
        match CreateMutexW(None, false, INSTANCE_MUTEX) {
            Ok(_handle) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => true,
        }
    }
}

fn run_inner() -> windows::core::Result<()> {
    // SAFETY: configuração do processo e do COM na thread principal, antes de qualquer janela.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    logf!("vholumin {} iniciado", crate::VERSION);

    let hwnd = create_window()?;
    TASKBAR_CREATED.store(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }, Ordering::Relaxed);

    let mut app = App {
        hwnd,
        tray: Tray::new(hwnd),
        config: Config::default(),
        problems: Vec::new(),
        hotkey: HotkeyState::Failed,
        steady: (IconState::Normal, String::new()),
        watch: None,
        generation: 0,
    };
    app.apply_config(Config::load());
    let (state, tip) = app.steady.clone();
    if !app.tray.show(state, &tip) {
        logf!("não foi possível colocar o ícone na bandeja");
    }
    APP.with(|cell| *cell.borrow_mut() = Some(app));

    let mut msg = MSG::default();
    // SAFETY: laço de mensagens padrão do Windows.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CoUninitialize();
    }
    logf!("vholumin encerrado");
    Ok(())
}

fn create_window() -> windows::core::Result<HWND> {
    // SAFETY: registro de classe e criação de uma janela que nunca é mostrada.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            WINDOW_CLASS,
            w!("vholumin"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY if wparam.0 == HOTKEY_ID as usize => with_app(App::on_hotkey),
        WM_TRAY => {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_CONTEXTMENU || event == NIN_SELECT || event == (NIN_SELECT | NINF_KEY) {
                show_menu(hwnd);
            }
        }
        WM_AUDIO_EVENT => with_app(|app| app.on_audio_event(wparam, lparam)),
        WM_OPEN_SETTINGS => open_config_file(),
        WM_TIMER if wparam.0 == FLASH_TIMER => {
            unsafe {
                let _ = KillTimer(Some(hwnd), FLASH_TIMER);
            }
            with_app(App::refresh_icon);
        }
        WM_DESTROY => {
            // Solta a bind, para de acompanhar o áudio e tira o ícone antes de sair.
            let app = APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut a| a.take()));
            drop(app);
            unsafe { PostQuitMessage(0) };
        }
        _ if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) => with_app(|app| app.tray.recreate()),
        // SAFETY: tratamento padrão para tudo o que não é nosso.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}

/// Executa `f` com o estado do programa. Se já estiver em uso (mensagem reentrante), ignora.
fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(app) = guard.as_mut()
        {
            f(app);
        }
    });
}

impl App {
    /// Aplica uma configuração recém-lida (ao abrir e ao recarregar).
    fn apply_config(&mut self, loaded: Loaded) {
        for problem in &loaded.problems {
            logf!("config: {problem}");
        }
        let paused = self.hotkey == HotkeyState::Paused;
        self.unregister_hotkey();
        self.config = loaded.config;
        self.problems = loaded.problems;
        if paused {
            self.hotkey = HotkeyState::Paused;
        } else {
            self.register_hotkey();
        }
        if let Err(e) = autostart::sync(self.config.start_with_windows) {
            logf!("iniciar com o Windows: não foi possível ajustar ({e})");
        }
        self.watch = None;
        self.steady = self.idle_status();
        let c = &self.config;
        logf!(
            "configuração: atalho {}, volumes {}%–{}%, modo {}, {} jogo(s) na lista, iniciar com o Windows: {}",
            c.hotkey.label(),
            c.min_pct,
            c.max_pct,
            c.mode.key(),
            c.games.len(),
            if c.start_with_windows { "sim" } else { "não" }
        );
    }

    fn register_hotkey(&mut self) {
        let hotkey = self.config.hotkey;
        let label = hotkey.label();
        let result = unsafe {
            RegisterHotKey(Some(self.hwnd), HOTKEY_ID, HOT_KEY_MODIFIERS(hotkey.modifiers) | MOD_NOREPEAT, hotkey.vk)
        };
        self.hotkey = match result {
            Ok(()) => {
                logf!("bind {label} registrada");
                HotkeyState::Registered
            }
            Err(e) if e.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() => {
                logf!("bind {label} indisponível: já está em uso por outro programa");
                HotkeyState::Taken
            }
            Err(e) => {
                logf!("bind {label} não pôde ser registrada: {e}");
                HotkeyState::Failed
            }
        };
    }

    fn unregister_hotkey(&mut self) {
        if self.hotkey == HotkeyState::Registered {
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_ID);
            }
            self.hotkey = HotkeyState::Failed;
        }
    }

    /// Ícone e texto quando nada foi alternado ainda.
    fn idle_status(&self) -> (IconState, String) {
        let label = self.config.hotkey.label();
        match self.hotkey {
            HotkeyState::Paused => (IconState::Unavailable, "vholumin — atalho pausado".into()),
            HotkeyState::Taken => {
                (IconState::Unavailable, format!("vholumin — {label} já está em uso por outro programa"))
            }
            HotkeyState::Failed => (IconState::Unavailable, format!("vholumin — {label} não pôde ser registrada")),
            HotkeyState::Registered if !self.problems.is_empty() => {
                (IconState::Normal, "vholumin — erro no config.toml (veja o menu)".into())
            }
            HotkeyState::Registered => match self.config.mode {
                Mode::Focus => (IconState::Normal, format!("vholumin — {label} alterna o volume do app em foco")),
                Mode::List => (IconState::Normal, format!("vholumin — {label} alterna o volume dos jogos da lista")),
            },
        }
    }

    fn on_hotkey(&mut self) {
        let target = match self.config.mode {
            Mode::Focus => Target::Focus(foreground_pid()),
            Mode::List => Target::List(&self.config.games),
        };
        match toggle::toggle(target, self.config.range()) {
            Ok(Outcome::Changed(report)) => {
                let names = report.target_names();
                let state = match report.toggle {
                    Toggle::ToMin => IconState::Lowered,
                    Toggle::ToMax => IconState::Normal,
                };
                let tip = format!("vholumin — {names} em {}%", pct(report.level));
                self.generation += 1;
                let controls = report.changes.iter().map(|c| c.session.control());
                self.watch = Some(Watch::new(self.hwnd, self.generation, names, controls));
                self.set_steady(state, tip);
            }
            Ok(Outcome::NotFound(what)) => self.flash(format!("vholumin — {what}")),
            Err(e) => {
                logf!("erro ao alternar: {e}");
                self.flash("vholumin — erro ao alternar o volume".into());
            }
        }
    }

    /// Aviso de áudio de uma sessão alternada: volume mudado por fora ou jogo fechado.
    fn on_audio_event(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let (generation, kind) = events::decode(wparam);
        let Some(watch) = self.watch.as_mut().filter(|w| w.generation == generation) else { return };
        let names = watch.names.clone();
        match kind {
            EVENT_VOLUME => {
                let volume = f32::from_bits(lparam.0 as u32);
                logf!("volume de {names} mudou por fora do vholumin: {}%", pct(volume));
                let state =
                    if volume <= self.config.range().min + TOLERANCE { IconState::Lowered } else { IconState::Normal };
                self.set_steady(state, format!("vholumin — {names} em {}%", pct(volume)));
            }
            EVENT_GONE => {
                if watch.mark_gone(lparam.0 as usize) {
                    logf!("áudio de {names} encerrado");
                    self.watch = None;
                    let (state, tip) = self.idle_status();
                    self.set_steady(state, tip);
                }
            }
            _ => {}
        }
    }

    fn on_command(&mut self, command: i32) {
        match command {
            CMD_MODE_FOCUS => self.set_mode(Mode::Focus),
            CMD_MODE_LIST => self.set_mode(Mode::List),
            CMD_PAUSE => self.toggle_pause(),
            CMD_AUTOSTART => {
                self.config.start_with_windows = !self.config.start_with_windows;
                self.save_config();
                if let Err(e) = autostart::sync(self.config.start_with_windows) {
                    logf!("iniciar com o Windows: não foi possível ajustar ({e})");
                }
            }
            CMD_RELOAD => {
                logf!("recarregando configurações");
                self.apply_config(Config::load());
                self.refresh_icon();
            }
            _ => {}
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.config.mode == mode {
            return;
        }
        self.config.mode = mode;
        logf!("modo: {}", mode.key());
        self.save_config();
        self.watch = None;
        let (state, tip) = self.idle_status();
        self.set_steady(state, tip);
    }

    fn toggle_pause(&mut self) {
        if self.hotkey == HotkeyState::Paused {
            logf!("atalho retomado");
            self.register_hotkey();
        } else {
            logf!("atalho pausado");
            self.unregister_hotkey();
            self.hotkey = HotkeyState::Paused;
        }
        self.watch = None;
        let (state, tip) = self.idle_status();
        self.set_steady(state, tip);
    }

    /// Salva a configuração atual (usado pelas opções do menu). O arquivo é reescrito.
    fn save_config(&mut self) {
        match self.config.save() {
            Ok(()) => self.problems.clear(),
            Err(e) => logf!("não foi possível salvar o config.toml: {e}"),
        }
    }

    fn set_steady(&mut self, state: IconState, tip: String) {
        self.steady = (state, tip);
        self.refresh_icon();
    }

    /// Mostra o ícone "de verdade" (cancela uma piscada em andamento).
    fn refresh_icon(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), FLASH_TIMER);
        }
        let (state, tip) = self.steady.clone();
        self.tray.set(state, &tip);
    }

    /// Mostra o ícone de "nada encontrado" por um instante e depois volta ao normal.
    fn flash(&mut self, tip: String) {
        self.tray.set(IconState::NotFound, &tip);
        unsafe { SetTimer(Some(self.hwnd), FLASH_TIMER, FLASH_MS, None) };
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.unregister_hotkey();
    }
}

/// PID do processo dono da janela em foco (0 se não houver). Não abre o processo.
fn foreground_pid() -> u32 {
    let mut pid = 0;
    unsafe {
        let window = GetForegroundWindow();
        if !window.is_invalid() {
            GetWindowThreadProcessId(window, Some(&mut pid));
        }
    }
    pid
}

/// O que o menu precisa mostrar (copiado do estado para não segurá-lo com o menu aberto).
struct MenuInfo {
    status: String,
    problem: Option<String>,
    mode: Mode,
    games: usize,
    paused: bool,
    autostart: bool,
}

fn show_menu(hwnd: HWND) {
    let mut info = None;
    with_app(|app| {
        let label = app.config.hotkey.label();
        info = Some(MenuInfo {
            status: match app.hotkey {
                HotkeyState::Registered => format!("Atalho: {label}"),
                HotkeyState::Paused => format!("Atalho: {label} (pausado)"),
                HotkeyState::Taken => format!("Atalho: {label} (em uso por outro programa)"),
                HotkeyState::Failed => format!("Atalho: {label} (não registrado)"),
            },
            problem: app.problems.first().cloned(),
            mode: app.config.mode,
            games: app.config.games.len(),
            paused: app.hotkey == HotkeyState::Paused,
            autostart: app.config.start_with_windows,
        });
    });
    let Some(info) = info else { return };

    // SAFETY: menu criado, mostrado e destruído aqui. Nenhum estado do programa fica emprestado
    // enquanto o menu está aberto (o menu processa mensagens, inclusive a bind).
    let command = unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        // "&" marca a letra de atalho do item; em textos variáveis vira "&&" (um & literal).
        let checked = |on: bool| if on { MF_CHECKED } else { MENU_ITEM_FLAGS(0) };
        let plain = |text: &str| text.replace('&', "&&");
        add(menu, MF_GRAYED, 0, &format!("vholumin {}", crate::VERSION));
        add(menu, MF_GRAYED, 0, &plain(&info.status));
        if let Some(problem) = &info.problem {
            let short: String = problem.chars().take(70).collect();
            add(menu, MF_GRAYED, 0, &plain(&format!("Erro no config.toml: {short}")));
        }
        separator(menu);
        add(menu, checked(info.mode == Mode::Focus), CMD_MODE_FOCUS, "Modo: app em &foco");
        add(menu, checked(info.mode == Mode::List), CMD_MODE_LIST, &format!("Modo: &lista de jogos ({})", info.games));
        separator(menu);
        add(menu, checked(info.paused), CMD_PAUSE, "&Pausar atalho");
        add(menu, checked(info.autostart), CMD_AUTOSTART, "&Iniciar com o Windows");
        separator(menu);
        add(menu, MENU_ITEM_FLAGS(0), CMD_EDIT_CONFIG, "&Editar configurações…");
        add(menu, MENU_ITEM_FLAGS(0), CMD_RELOAD, "&Recarregar configurações");
        add(menu, MENU_ITEM_FLAGS(0), CMD_OPEN_LOGS, "Abrir pasta de lo&gs");
        separator(menu);
        add(menu, MENU_ITEM_FLAGS(0), CMD_EXIT, "&Sair");

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let command =
            TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, None, hwnd, None).0;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        command
    };

    match command {
        0 => {}
        CMD_EDIT_CONFIG => open_config_file(),
        CMD_OPEN_LOGS => open_logs_folder(),
        CMD_EXIT => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        other => with_app(|app| app.on_command(other)),
    }
}

unsafe fn add(menu: HMENU, flags: MENU_ITEM_FLAGS, id: i32, text: &str) {
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING | flags, id as usize, &HSTRING::from(text));
    }
}

unsafe fn separator(menu: HMENU) {
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
}

/// Abre o config.toml no Bloco de Notas (criando com os padrões, se não existir).
fn open_config_file() {
    let Some(path) = Config::path() else { return };
    if !path.exists()
        && let Err(e) = Config::default().save()
    {
        logf!("não foi possível criar o config.toml: {e}");
        return;
    }
    let args = HSTRING::from(format!("\"{}\"", path.display()));
    unsafe {
        ShellExecuteW(None, w!("open"), w!("notepad.exe"), &args, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

fn open_logs_folder() {
    let Some(dir) = crate::log::data_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(dir.as_path()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
