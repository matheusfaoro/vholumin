//! Janela de configurações: tema escuro, desenhada por código com Direct2D (sem os controles
//! padrão do Windows).
//!
//! Roda num processo separado (`vholumin.exe --configuracoes`), aberto pela cópia da bandeja:
//! quando a janela fecha o processo termina e devolve toda a memória (o Direct2D e as fontes
//! nunca são carregados na cópia da bandeja). Salvar grava o config.toml; a cópia da bandeja
//! relê o arquivo quando este processo termina.
//!
//! Enquanto ela está aberta a bind fica suspensa (senão apertar a combinação atual no campo
//! de atalho alternaria o volume do jogo em vez de ser capturada).
//!
//! Organização: `Ui` guarda o estado e trata as mensagens. O que faz o Windows chamar a
//! janela de volta ou abre outro laço de mensagens (menu, escolher arquivo, redimensionar,
//! fechar, capturar o mouse) vira um `Effect`, executado depois de soltar o estado — senão o
//! Windows entraria de novo na janela com o estado ainda emprestado.

use std::cell::RefCell;

use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U;
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1CreateFactory, ID2D1Factory,
    ID2D1HwndRenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, EndPaint, GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow, PAINTSTRUCT, ScreenToClient,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, GetSystemMetricsForDpi, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, ReleaseCapture, SetCapture, TME_LEAVE,
    TRACKMOUSEEVENT, TrackMouseEvent, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CS_HREDRAW, CS_VREDRAW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, FindWindowW, GetClientRect, GetCursorPos, GetMessageW, GetWindowRect, HICON,
    ICON_BIG, ICON_SMALL, IDC_ARROW, IsIconic, LoadCursorW, MF_GRAYED, MF_STRING, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, SM_CXICON, SM_CXSMICON, SW_RESTORE, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
    SetForegroundWindow, SetWindowPos, ShowWindow, TPM_LEFTALIGN, TPM_RETURNCMD, TranslateMessage,
    TPM_TOPALIGN, TrackPopupMenu, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CAPTURECHANGED, WM_CLOSE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETICON, WM_SIZE, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSW,
    WS_CAPTION, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::{HRESULT, HSTRING, PCWSTR, PWSTR, Result, w};

use crate::audio;
use crate::config::{self, Config, Mode};
use crate::hotkey::{Hotkey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
use crate::icons::{self, IconState};
use crate::paint::{Align, Fonts, Painter, Rect, glyph, theme};
use crate::processes::ProcessTable;
use crate::target::same_exe;
use crate::theme as window_theme;

const CLASS: PCWSTR = w!("vholumin.configuracoes");
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0);
const D2DERR_RECREATE_TARGET: HRESULT = HRESULT(0x8899_000C_u32 as i32);
/// Número usado só para testar se uma combinação está livre (registra e solta na hora).
const TEST_HOTKEY_ID: i32 = 0xBFFF;

// Layout, em DIPs.
const WIDTH: f32 = 520.0;
const MARGIN: f32 = 20.0;
const PAD: f32 = 16.0;
const KEYBOX_W: f32 = 230.0;
const ROW_H: f32 = 30.0;
const LIST_ROWS: usize = 4;

// Teclas (winuser.h).
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_DELETE: u32 = 0x2E;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_F10: u32 = 0x79;

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Widget {
    KeyBox,
    MinSlider,
    MaxSlider,
    ModeFocus,
    ModeList,
    List,
    Add,
    Browse,
    Remove,
    Toggle,
    Cancel,
    Save,
}

use Widget::*;

const ALL_WIDGETS: [Widget; 12] =
    [KeyBox, MinSlider, MaxSlider, ModeFocus, ModeList, List, Add, Browse, Remove, Toggle, Cancel, Save];

/// Ações executadas depois de soltar o estado da janela.
enum Effect {
    /// Ajustar a altura da janela ao conteúdo (lista aberta/fechada).
    Resize,
    /// Mover/redimensionar para o retângulo sugerido pelo Windows (mudança de escala).
    SetRect(RECT),
    Capture,
    Release,
    TrackLeave,
    PlayingMenu(Rect),
    Browse,
    Close(Option<Config>),
    Dispose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusKind {
    Ok,
    Info,
    Error,
}

#[derive(Clone, Copy)]
struct SliderRow {
    label: Rect,
    slider: Rect,
    value: Rect,
}

struct Layout {
    card_hotkey: Rect,
    keybox: Rect,
    hotkey_status: Rect,
    card_volume: Rect,
    min: SliderRow,
    max: SliderRow,
    card_mode: Rect,
    seg_focus: Rect,
    seg_list: Rect,
    mode_desc: Rect,
    list: Option<Rect>,
    add: Option<Rect>,
    browse: Option<Rect>,
    remove: Option<Rect>,
    save_error: Option<Rect>,
    toggle: Rect,
    toggle_label: Rect,
    cancel: Rect,
    save: Rect,
    height: f32,
}

impl Layout {
    fn rect(&self, widget: Widget) -> Option<Rect> {
        match widget {
            KeyBox => Some(self.keybox),
            MinSlider => Some(self.min.slider),
            MaxSlider => Some(self.max.slider),
            ModeFocus => Some(self.seg_focus),
            ModeList => Some(self.seg_list),
            List => self.list,
            Add => self.add,
            Browse => self.browse,
            Remove => self.remove,
            Toggle => Some(self.toggle.union(self.toggle_label)),
            Cancel => Some(self.cancel),
            Save => Some(self.save),
        }
    }
}

/// Onde fica cada coisa. A lista e seus botões só existem no modo lista.
fn layout(list_mode: bool, save_error: bool) -> Layout {
    let (x, w) = (MARGIN, WIDTH - 2.0 * MARGIN);
    let (inner_x, inner_w) = (x + PAD, w - 2.0 * PAD);
    let mut y = 16.0;

    let card_hotkey = Rect::new(x, y, w, 88.0);
    let keybox = Rect::new(x + w - PAD - KEYBOX_W, y + 14.0, KEYBOX_W, 40.0);
    let hotkey_status = Rect::new(inner_x, y + 62.0, inner_w, 18.0);
    y += 88.0 + 10.0;

    let card_volume = Rect::new(x, y, w, 112.0);
    let row = |ry: f32| SliderRow {
        label: Rect::new(inner_x, ry, 64.0, 24.0),
        slider: Rect::new(inner_x + 72.0, ry, inner_w - 72.0 - 56.0, 24.0),
        value: Rect::new(x + w - PAD - 48.0, ry, 48.0, 24.0),
    };
    let (min, max) = (row(y + 46.0), row(y + 78.0));
    y += 112.0 + 10.0;

    let mode_h = if list_mode { 280.0 } else { 108.0 };
    let card_mode = Rect::new(x, y, w, mode_h);
    let seg_w = inner_w / 2.0;
    let seg_focus = Rect::new(inner_x, y + 41.0, seg_w, 36.0);
    let seg_list = Rect::new(inner_x + seg_w, y + 41.0, seg_w, 36.0);
    let mode_desc = Rect::new(inner_x, y + 82.0, inner_w, 18.0);
    let (list, add, browse, remove) = if list_mode {
        let by = y + 236.0;
        (
            Some(Rect::new(inner_x, y + 108.0, inner_w, ROW_H * LIST_ROWS as f32)),
            Some(Rect::new(inner_x, by, 200.0, 32.0)),
            Some(Rect::new(inner_x + 208.0, by, 132.0, 32.0)),
            Some(Rect::new(inner_x + 348.0, by, 100.0, 32.0)),
        )
    } else {
        (None, None, None, None)
    };
    y += mode_h + 14.0;

    let save_error = if save_error {
        let r = Rect::new(x, y, w, 18.0);
        y += 26.0;
        Some(r)
    } else {
        None
    };

    let toggle = Rect::new(x, y + 7.0, 40.0, 20.0);
    let toggle_label = Rect::new(x + 50.0, y, 200.0, 34.0);
    let save = Rect::new(WIDTH - MARGIN - 110.0, y, 110.0, 34.0);
    let cancel = Rect::new(save.x - 8.0 - 110.0, y, 110.0, 34.0);

    Layout {
        card_hotkey,
        keybox,
        hotkey_status,
        card_volume,
        min,
        max,
        card_mode,
        seg_focus,
        seg_list,
        mode_desc,
        list,
        add,
        browse,
        remove,
        save_error,
        toggle,
        toggle_label,
        cancel,
        save,
        height: y + 34.0 + 18.0,
    }
}

/// Valor de 0 a 100 do slider na posição horizontal `px`.
fn slider_value(slider: Rect, px: f32) -> u8 {
    let (x0, x1) = (slider.x + 10.0, slider.right() - 10.0);
    (((px - x0) / (x1 - x0)).clamp(0.0, 1.0) * 100.0).round() as u8
}

/// Novo (mínimo, máximo) quando um dos dois muda: empurra o outro para manter mínimo < máximo.
fn adjust_range(min: u8, max: u8, changing_min: bool, value: u8) -> (u8, u8) {
    if changing_min {
        let v = value.min(99);
        (v, max.max(v + 1))
    } else {
        let v = value.clamp(1, 100);
        (min.min(v - 1), v)
    }
}

struct Ui {
    hwnd: HWND,
    dpi: u32,
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    fonts: Fonts,
    target: Option<ID2D1HwndRenderTarget>,
    icons: [HICON; 2],

    /// A bind válida escolhida (ou o motivo de não ser válida).
    hotkey: std::result::Result<Hotkey, String>,
    /// O que o campo mostra (pode ser uma combinação inválida que acabou de ser tentada).
    shown: Hotkey,
    hotkey_touched: bool,
    hotkey_status: (StatusKind, String),
    capturing: bool,
    held: u32,
    min: u8,
    max: u8,
    mode: Mode,
    games: Vec<String>,
    selected: Option<usize>,
    scroll: usize,
    autostart: bool,
    save_error: Option<String>,

    hover: Option<Widget>,
    hover_row: Option<usize>,
    pressed: Option<Widget>,
    focus: Option<Widget>,
    /// Mostra o anel de foco só depois que o teclado foi usado (como o Windows faz).
    keyboard_focus: bool,
    dragging: Option<Widget>,
    tracking: bool,
}

/// A janela de configurações aberta (em qualquer processo), se houver.
fn existing_window() -> Option<HWND> {
    unsafe { FindWindowW(CLASS, PCWSTR::null()) }.ok().filter(|hwnd| !hwnd.is_invalid())
}

/// Se a janela já está aberta, traz para a frente e devolve `true`.
pub fn bring_existing_to_front() -> bool {
    let Some(hwnd) = existing_window() else { return false };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

/// Pede para a janela aberta (se houver) fechar sem salvar.
pub fn close_existing() {
    if let Some(hwnd) = existing_window() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// O processo `vholumin.exe --configuracoes`: mostra a janela e termina quando ela fecha.
pub fn run() -> i32 {
    if bring_existing_to_front() {
        return 0;
    }
    unsafe {
        // COM: lista de sessões de áudio ("Adicionar tocando agora") e a janela de arquivos.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    window_theme::enable_dark_menus();
    if let Err(e) = open(Config::load().config) {
        logf!("não foi possível abrir a janela de configurações: {e}");
        return 1;
    }
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CoUninitialize();
    }
    0
}

/// Cria e mostra a janela com a configuração dada.
fn open(config: Config) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&class); // na segunda abertura já está registrada; tudo bem

        // Abre no monitor do mouse, um pouco acima do centro.
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let (mut dpi, mut dpi_y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
        let (w, h) = window_size(layout(config.mode == Mode::List, false).height, dpi);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let work = info.rcWork;
        let x = work.left + ((work.right - work.left) - w).max(0) / 2;
        let y = work.top + ((work.bottom - work.top) - h).max(0) / 3;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS,
            w!("vholumin — configurações"),
            STYLE,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        window_theme::dark_title_bar(hwnd);
        match Ui::new(hwnd, config) {
            Ok(ui) => UI.with(|cell| *cell.borrow_mut() = Some(ui)),
            Err(e) => {
                let _ = DestroyWindow(hwnd);
                return Err(e);
            }
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

/// Tamanho da janela (com barra de título e bordas) para uma área útil de WIDTH × `height`.
fn window_size(height: f32, dpi: u32) -> (i32, i32) {
    let scale = dpi as f32 / 96.0;
    let mut rect = RECT { left: 0, top: 0, right: (WIDTH * scale).round() as i32, bottom: (height * scale).round() as i32 };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rect, STYLE, false, WINDOW_EX_STYLE::default(), dpi);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with(|cell| cell.try_borrow_mut().ok().and_then(|mut guard| guard.as_mut().map(f)))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_ERASEBKGND {
        return LRESULT(1); // o Direct2D pinta tudo; apagar antes só causaria piscadas
    }
    let handled = with_ui(|ui| {
        let mut effects = Vec::new();
        let result = ui.handle(msg, wparam, lparam, &mut effects);
        (result, effects)
    });
    match handled {
        Some((result, effects)) => {
            run_effects(hwnd, effects);
            result.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) })
        }
        None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn run_effects(hwnd: HWND, effects: Vec<Effect>) {
    for effect in effects {
        unsafe {
            match effect {
                Effect::Resize => resize_window(hwnd),
                Effect::SetRect(r) => {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                Effect::Capture => {
                    SetCapture(hwnd);
                }
                Effect::Release => {
                    let _ = ReleaseCapture();
                }
                Effect::TrackLeave => {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                }
                Effect::PlayingMenu(button) => playing_menu(hwnd, button),
                Effect::Browse => browse(hwnd),
                Effect::Close(saved) => {
                    if let Some(config) = saved {
                        logf!(
                            "configurações salvas pela janela: atalho {}, volumes {}%–{}%, modo {}",
                            config.hotkey.label(),
                            config.min_pct,
                            config.max_pct,
                            config.mode.key()
                        );
                    }
                    let _ = DestroyWindow(hwnd);
                    return;
                }
                Effect::Dispose => {
                    let ui = UI.with(|cell| cell.borrow_mut().take());
                    drop(ui);
                    PostQuitMessage(0); // fim do processo da janela
                }
            }
        }
    }
}

/// Ajusta a altura ao conteúdo, mantendo a janela dentro da área útil da tela.
fn resize_window(hwnd: HWND) {
    let Some((height, dpi)) = with_ui(|ui| (ui.layout().height, ui.dpi)) else { return };
    let (w, h) = window_size(height, dpi);
    unsafe {
        let mut current = RECT::default();
        let _ = GetWindowRect(hwnd, &mut current);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info);
        let work = info.rcWork;
        let top = if current.top + h > work.bottom { (work.bottom - h).max(work.top) } else { current.top };
        let _ = SetWindowPos(hwnd, None, current.left, top, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

impl Ui {
    fn new(hwnd: HWND, config: Config) -> Result<Self> {
        let (d2d, dw): (ID2D1Factory, IDWriteFactory) = unsafe {
            (D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?, DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?)
        };
        let fonts = Fonts::new(&dw)?;
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let mut ui = Self {
            hwnd,
            dpi,
            d2d,
            dw,
            fonts,
            target: None,
            icons: set_icons(hwnd, dpi),
            hotkey: Ok(config.hotkey),
            shown: config.hotkey,
            hotkey_touched: false,
            hotkey_status: (StatusKind::Info, String::new()),
            capturing: false,
            held: 0,
            min: config.min_pct,
            max: config.max_pct,
            mode: config.mode,
            games: config.games,
            selected: None,
            scroll: 0,
            autostart: config.start_with_windows,
            save_error: None,
            hover: None,
            hover_row: None,
            pressed: None,
            focus: None,
            keyboard_focus: false,
            dragging: None,
            tracking: false,
        };
        ui.update_hotkey_status();
        Ok(ui)
    }

    fn layout(&self) -> Layout {
        layout(self.mode == Mode::List, self.save_error.is_some())
    }

    fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// Posição do mouse (em `lparam`, pixels da área útil) em DIPs.
    fn dip(&self, lparam: LPARAM) -> (f32, f32) {
        let x = (lparam.0 & 0xFFFF) as u16 as i16 as f32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
        let scale = 96.0 / self.dpi as f32;
        (x * scale, y * scale)
    }

    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM, fx: &mut Vec<Effect>) -> Option<LRESULT> {
        let done = Some(LRESULT(0));
        match msg {
            WM_PAINT => {
                self.paint();
                done
            }
            WM_SIZE => {
                if let Some(target) = &self.target {
                    let size = D2D_SIZE_U {
                        width: (lparam.0 & 0xFFFF) as u32,
                        height: ((lparam.0 >> 16) & 0xFFFF) as u32,
                    };
                    unsafe {
                        let _ = target.Resize(&size);
                    }
                }
                self.invalidate();
                done
            }
            WM_DPICHANGED => {
                self.dpi = ((wparam.0 >> 16) & 0xFFFF) as u32;
                if let Some(target) = &self.target {
                    unsafe { target.SetDpi(self.dpi as f32, self.dpi as f32) };
                }
                // SAFETY: no WM_DPICHANGED, lparam aponta para o retângulo sugerido.
                fx.push(Effect::SetRect(unsafe { *(lparam.0 as *const RECT) }));
                done
            }
            WM_MOUSEMOVE => {
                self.on_mouse_move(lparam, fx);
                done
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                self.hover = None;
                self.hover_row = None;
                self.invalidate();
                done
            }
            WM_LBUTTONDOWN => {
                self.on_mouse_down(lparam, fx);
                done
            }
            WM_LBUTTONUP => {
                self.on_mouse_up(lparam, fx);
                done
            }
            WM_MOUSEWHEEL => {
                self.on_wheel(wparam, lparam);
                done
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wparam.0 as u32;
                if self.capturing {
                    self.on_capture_key(vk, true);
                    return done;
                }
                if vk == VK_MENU || vk == VK_F10 {
                    return done; // a janela não tem menu: nada de "modo menu" ao soltar o Alt
                }
                if msg == WM_KEYDOWN && self.on_key(vk, fx) { done } else { None }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                let vk = wparam.0 as u32;
                if self.capturing {
                    self.on_capture_key(vk, false);
                    return done;
                }
                if vk == VK_MENU || vk == VK_F10 { done } else { None }
            }
            // Alt+letra sem menu faz "bip"; só deixa passar o Alt+Espaço (menu da janela).
            WM_SYSCHAR => {
                if wparam.0 == ' ' as usize && !self.capturing { None } else { done }
            }
            WM_KILLFOCUS => {
                self.stop_capture();
                self.invalidate();
                None
            }
            WM_CAPTURECHANGED => {
                self.dragging = None;
                self.pressed = None;
                self.invalidate();
                None
            }
            WM_CLOSE => {
                fx.push(Effect::Close(None));
                done
            }
            WM_DESTROY => {
                fx.push(Effect::Dispose);
                None
            }
            _ => None,
        }
    }

    // ---- mouse ----

    fn hit(&self, px: f32, py: f32) -> Option<Widget> {
        let layout = self.layout();
        ALL_WIDGETS.into_iter().find(|&w| layout.rect(w).is_some_and(|r| r.contains(px, py)))
    }

    fn row_at(&self, px: f32, py: f32) -> Option<usize> {
        let list = self.layout().list?;
        if !list.contains(px, py) {
            return None;
        }
        let row = self.scroll + ((py - list.y) / ROW_H) as usize;
        (row < self.games.len()).then_some(row)
    }

    fn on_mouse_move(&mut self, lparam: LPARAM, fx: &mut Vec<Effect>) {
        let (px, py) = self.dip(lparam);
        if !self.tracking {
            self.tracking = true;
            fx.push(Effect::TrackLeave);
        }
        if let Some(slider) = self.dragging {
            if let Some(r) = self.layout().rect(slider) {
                self.set_slider(slider, slider_value(r, px));
            }
            self.invalidate();
            return;
        }
        let (hover, row) = (self.hit(px, py), self.row_at(px, py));
        if hover != self.hover || row != self.hover_row {
            self.hover = hover;
            self.hover_row = row;
            self.invalidate();
        }
    }

    fn on_mouse_down(&mut self, lparam: LPARAM, fx: &mut Vec<Effect>) {
        let (px, py) = self.dip(lparam);
        self.keyboard_focus = false;
        let hit = self.hit(px, py).filter(|&w| self.enabled(w));
        if hit != Some(KeyBox) {
            self.stop_capture();
        }
        self.pressed = hit;
        if let Some(widget) = hit {
            fx.push(Effect::Capture);
            self.focus = Some(widget);
            match widget {
                MinSlider | MaxSlider => {
                    self.dragging = Some(widget);
                    if let Some(r) = self.layout().rect(widget) {
                        self.set_slider(widget, slider_value(r, px));
                    }
                }
                List => {
                    if let Some(row) = self.row_at(px, py) {
                        self.selected = Some(row);
                    }
                }
                KeyBox => self.start_capture(),
                _ => {}
            }
        }
        self.invalidate();
    }

    fn on_mouse_up(&mut self, lparam: LPARAM, fx: &mut Vec<Effect>) {
        let (px, py) = self.dip(lparam);
        let pressed = self.pressed.take();
        self.dragging = None;
        if pressed.is_some() {
            fx.push(Effect::Release);
        }
        if let Some(widget) = pressed
            && self.hit(px, py) == Some(widget)
            && !matches!(widget, MinSlider | MaxSlider | List | KeyBox)
        {
            self.activate(widget, fx);
        }
        self.invalidate();
    }

    fn on_wheel(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16;
        let mut point = POINT {
            x: (lparam.0 & 0xFFFF) as u16 as i16 as i32,
            y: ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
        };
        unsafe {
            let _ = ScreenToClient(self.hwnd, &mut point);
        }
        let scale = 96.0 / self.dpi as f32;
        let (px, py) = (point.x as f32 * scale, point.y as f32 * scale);
        if let Some(list) = self.layout().list
            && list.contains(px, py)
        {
            let max_scroll = self.games.len().saturating_sub(LIST_ROWS);
            self.scroll = if delta > 0 { self.scroll.saturating_sub(1) } else { (self.scroll + 1).min(max_scroll) };
            self.hover_row = self.row_at(px, py);
            self.invalidate();
        }
    }

    // ---- teclado ----

    fn on_key(&mut self, vk: u32, fx: &mut Vec<Effect>) -> bool {
        let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
        match vk {
            VK_TAB => {
                self.keyboard_focus = true;
                self.move_focus(if shift { -1 } else { 1 });
            }
            VK_ESCAPE => fx.push(Effect::Close(None)),
            VK_RETURN => match self.focus {
                Some(w) if !matches!(w, MinSlider | MaxSlider | List) => self.activate(w, fx),
                _ => self.activate(Save, fx),
            },
            VK_SPACE => {
                if let Some(w) = self.focus {
                    self.activate(w, fx);
                }
            }
            _ => {
                self.keyboard_focus = true;
                match self.focus {
                    Some(w @ (MinSlider | MaxSlider)) => {
                        let current = i32::from(if w == MinSlider { self.min } else { self.max });
                        let next = match vk {
                            VK_LEFT | VK_DOWN => current - 1,
                            VK_RIGHT | VK_UP => current + 1,
                            VK_PRIOR => current + 10,
                            VK_NEXT => current - 10,
                            VK_HOME => 0,
                            VK_END => 100,
                            _ => return false,
                        };
                        self.set_slider(w, next.clamp(0, 100) as u8);
                    }
                    Some(ModeFocus | ModeList) => match vk {
                        VK_LEFT => self.set_mode(Mode::Focus, fx),
                        VK_RIGHT => self.set_mode(Mode::List, fx),
                        _ => return false,
                    },
                    Some(List) => match vk {
                        VK_UP => self.move_selection(-1),
                        VK_DOWN => self.move_selection(1),
                        VK_DELETE => self.remove_selected(),
                        _ => return false,
                    },
                    _ => return false,
                }
            }
        }
        self.invalidate();
        true
    }

    fn tab_order(&self) -> Vec<Widget> {
        let list_mode = self.mode == Mode::List;
        let mode = if list_mode { ModeList } else { ModeFocus };
        let list_widgets: &[Widget] = if list_mode { &[List, Add, Browse, Remove] } else { &[] };
        [KeyBox, MinSlider, MaxSlider, mode]
            .iter()
            .chain(list_widgets)
            .chain(&[Toggle, Cancel, Save])
            .copied()
            .filter(|&w| self.enabled(w))
            .collect()
    }

    fn move_focus(&mut self, delta: i32) {
        self.stop_capture();
        let order = self.tab_order();
        if order.is_empty() {
            return;
        }
        let current = self.focus.and_then(|f| order.iter().position(|&w| w == f));
        let next = match current {
            Some(i) => (i as i32 + delta).rem_euclid(order.len() as i32) as usize,
            None if delta > 0 => 0,
            None => order.len() - 1,
        };
        self.focus = Some(order[next]);
    }

    // ---- captura da bind ----

    fn start_capture(&mut self) {
        self.capturing = true;
        self.held = 0;
        self.focus = Some(KeyBox);
    }

    fn stop_capture(&mut self) {
        self.capturing = false;
        self.held = 0;
    }

    fn on_capture_key(&mut self, vk: u32, down: bool) {
        let mods = current_modifiers();
        if !down {
            self.held = mods;
        } else {
            match vk {
                VK_SHIFT | VK_CONTROL | VK_MENU | VK_LWIN | VK_RWIN | 0xA0..=0xA5 => self.held = mods,
                VK_ESCAPE | VK_RETURN | VK_TAB if mods == 0 => self.stop_capture(),
                _ => {
                    self.shown = Hotkey { modifiers: mods, vk };
                    self.hotkey = Hotkey::new(mods, vk);
                    self.hotkey_touched = true;
                    self.stop_capture();
                    self.update_hotkey_status();
                }
            }
        }
        self.invalidate();
    }

    fn update_hotkey_status(&mut self) {
        self.hotkey_status = match &self.hotkey {
            Err(e) => (StatusKind::Error, capitalize(e)),
            Ok(h) if !self.hotkey_touched && h.modifiers & MOD_WIN != 0 => {
                (StatusKind::Info, format!("Atual: {} (definida no arquivo)", h.label()))
            }
            Ok(h) => match test_hotkey(self.hwnd, *h) {
                Ok(()) => (StatusKind::Ok, format!("{} está livre", h.label())),
                Err(e) => (StatusKind::Error, e),
            },
        };
    }

    // ---- ações ----

    fn enabled(&self, widget: Widget) -> bool {
        match widget {
            Remove => self.selected.is_some(),
            Save => self.can_save(),
            _ => true,
        }
    }

    fn can_save(&self) -> bool {
        self.hotkey.is_ok() && self.hotkey_status.0 != StatusKind::Error
    }

    fn activate(&mut self, widget: Widget, fx: &mut Vec<Effect>) {
        if !self.enabled(widget) {
            return;
        }
        match widget {
            KeyBox => self.start_capture(),
            ModeFocus => self.set_mode(Mode::Focus, fx),
            ModeList => self.set_mode(Mode::List, fx),
            Add => {
                if let Some(button) = self.layout().add {
                    fx.push(Effect::PlayingMenu(button));
                }
            }
            Browse => fx.push(Effect::Browse),
            Remove => self.remove_selected(),
            Toggle => self.autostart = !self.autostart,
            Cancel => fx.push(Effect::Close(None)),
            Save => self.save(fx),
            MinSlider | MaxSlider | List => {}
        }
        self.invalidate();
    }

    fn set_mode(&mut self, mode: Mode, fx: &mut Vec<Effect>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.focus = match self.focus {
            Some(ModeFocus | ModeList) => Some(if mode == Mode::List { ModeList } else { ModeFocus }),
            Some(List | Add | Browse | Remove) => None,
            other => other,
        };
        self.hover = None;
        fx.push(Effect::Resize);
    }

    fn set_slider(&mut self, widget: Widget, value: u8) {
        (self.min, self.max) = adjust_range(self.min, self.max, widget == MinSlider, value);
    }

    fn add_game(&mut self, entry: &str) {
        let Some(name) = config::normalize_game(entry) else { return };
        let index = match self.games.iter().position(|g| same_exe(g, &name)) {
            Some(existing) => existing,
            None => {
                self.games.push(name);
                self.games.len() - 1
            }
        };
        self.selected = Some(index);
        self.ensure_visible(index);
        self.invalidate();
    }

    fn remove_selected(&mut self) {
        let Some(index) = self.selected else { return };
        self.games.remove(index);
        self.selected = if self.games.is_empty() { None } else { Some(index.min(self.games.len() - 1)) };
        self.scroll = self.scroll.min(self.games.len().saturating_sub(LIST_ROWS));
    }

    fn move_selection(&mut self, delta: i32) {
        if self.games.is_empty() {
            return;
        }
        let next = match self.selected {
            Some(i) => (i as i32 + delta).clamp(0, self.games.len() as i32 - 1) as usize,
            None => 0,
        };
        self.selected = Some(next);
        self.ensure_visible(next);
    }

    fn ensure_visible(&mut self, index: usize) {
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + LIST_ROWS {
            self.scroll = index + 1 - LIST_ROWS;
        }
    }

    fn save(&mut self, fx: &mut Vec<Effect>) {
        let Ok(hotkey) = self.hotkey.clone() else { return };
        let config = Config {
            hotkey,
            min_pct: self.min,
            max_pct: self.max,
            mode: self.mode,
            games: self.games.clone(),
            start_with_windows: self.autostart,
        };
        match config.save() {
            Ok(()) => fx.push(Effect::Close(Some(config))),
            Err(e) => {
                if self.save_error.is_none() {
                    fx.push(Effect::Resize);
                }
                self.save_error = Some(format!("Não foi possível salvar: {e}"));
            }
        }
    }

    // ---- desenho ----

    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        unsafe { BeginPaint(self.hwnd, &mut ps) };
        if let Err(e) = self.draw() {
            if e.code() == D2DERR_RECREATE_TARGET {
                self.target = None;
                self.invalidate();
            } else {
                logf!("erro ao desenhar as configurações: {e}");
            }
        }
        unsafe {
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn render_target(&mut self) -> Result<ID2D1HwndRenderTarget> {
        if let Some(target) = &self.target {
            return Ok(target.clone());
        }
        let mut client = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut client)? };
        // Desenho por software: para uma janela deste tamanho é rápido do mesmo jeito, e não
        // carrega o driver da placa de vídeo (que ficaria na memória depois de fechar).
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            dpiX: self.dpi as f32,
            dpiY: self.dpi as f32,
            ..Default::default()
        };
        let hwnd_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: self.hwnd,
            pixelSize: D2D_SIZE_U { width: client.right as u32, height: client.bottom as u32 },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let target = unsafe { self.d2d.CreateHwndRenderTarget(&properties, &hwnd_properties)? };
        self.target = Some(target.clone());
        Ok(target)
    }

    fn draw(&mut self) -> Result<()> {
        let target = self.render_target()?;
        let p = Painter::new(&target, &self.dw)?;
        let l = self.layout();
        let f = &self.fonts;
        unsafe { target.BeginDraw() };
        p.clear(theme::BG);

        // Atalho
        self.card(&p, l.card_hotkey, "Atalho");
        let caption = Rect::new(l.card_hotkey.x + PAD, l.card_hotkey.y + 36.0, 200.0, 18.0);
        p.text("Clique e aperte a combinação", &f.caption, caption, theme::TEXT_2, Align::Left);
        self.draw_keybox(&p, l.keybox);
        let (kind, text) = &self.hotkey_status;
        self.draw_status(&p, l.hotkey_status, *kind, text);

        // Volume
        self.card(&p, l.card_volume, "Volume do jogo");
        for (row, name, value, widget) in [(l.min, "Mínimo", self.min, MinSlider), (l.max, "Máximo", self.max, MaxSlider)] {
            p.text(name, &f.body, row.label, theme::TEXT_2, Align::Left);
            self.draw_slider(&p, row.slider, value, widget);
            p.text(&format!("{value}%"), &f.body, row.value, theme::TEXT, Align::Right);
        }

        // Modo
        self.card(&p, l.card_mode, "Qual programa o atalho controla");
        self.draw_segmented(&p, &l);
        let description = match self.mode {
            Mode::Focus => "O atalho muda o volume do programa que estiver em foco.",
            Mode::List => "O atalho muda o volume dos jogos da lista, mesmo fora de foco.",
        };
        p.text(description, &f.caption, l.mode_desc, theme::TEXT_2, Align::Left);
        if let (Some(list), Some(add), Some(browse), Some(remove)) = (l.list, l.add, l.browse, l.remove) {
            self.draw_list(&p, list);
            self.draw_button(&p, add, Some(glyph::ADD), "Adicionar tocando agora", Add, false);
            self.draw_button(&p, browse, Some(glyph::SEARCH), "Procurar .exe", Browse, false);
            self.draw_button(&p, remove, Some(glyph::DELETE), "Remover", Remove, false);
        }

        // Rodapé
        if let (Some(r), Some(error)) = (l.save_error, &self.save_error) {
            self.draw_status(&p, r, StatusKind::Error, error);
        }
        self.draw_toggle(&p, l.toggle);
        p.text("Iniciar com o Windows", &f.body, l.toggle_label, theme::TEXT, Align::Left);
        self.draw_button(&p, l.cancel, None, "Cancelar", Cancel, false);
        self.draw_button(&p, l.save, None, "Salvar", Save, true);

        // Anel de foco (só depois de usar o teclado)
        if self.keyboard_focus
            && let Some(focus) = self.focus
            && let Some(r) = l.rect(focus)
        {
            let ring = if focus == Toggle { l.toggle } else { r };
            p.stroke(ring.inset(-3.0, -3.0), 7.0, theme::FOCUS, 2.0);
        }

        unsafe { target.EndDraw(None, None) }
    }

    fn card(&self, p: &Painter, r: Rect, title: &str) {
        p.fill(r, 8.0, theme::CARD);
        p.stroke(r, 8.0, theme::CARD_BORDER, 1.0);
        let title_rect = Rect::new(r.x + PAD, r.y + 14.0, r.w - 2.0 * PAD, 20.0);
        p.text(title, &self.fonts.strong, title_rect, theme::TEXT, Align::Left);
    }

    fn draw_status(&self, p: &Painter, r: Rect, kind: StatusKind, text: &str) {
        let (icon, color) = match kind {
            StatusKind::Ok => (glyph::CHECK, theme::SUCCESS),
            StatusKind::Info => (glyph::INFO, theme::TEXT_2),
            StatusKind::Error => (glyph::ERROR, theme::ERROR),
        };
        if text.is_empty() {
            return;
        }
        p.text(icon, &self.fonts.icon_small, Rect::new(r.x, r.y, 16.0, r.h), color, Align::Left);
        p.text(text, &self.fonts.caption, Rect::new(r.x + 20.0, r.y, r.w - 20.0, r.h), color, Align::Left);
    }

    fn draw_keybox(&self, p: &Painter, r: Rect) {
        p.fill(r, 6.0, theme::INPUT);
        if self.capturing {
            p.stroke(r, 6.0, theme::ACCENT, 2.0);
        } else {
            let border = if self.hover == Some(KeyBox) { theme::INPUT_BORDER_HOVER } else { theme::INPUT_BORDER };
            p.stroke(r, 6.0, border, 1.0);
        }

        let keys: Vec<String> = if self.capturing {
            if self.held == 0 {
                p.text("Aperte a combinação…", &self.fonts.body, r, theme::ACCENT, Align::Center);
                return;
            }
            let mut keys: Vec<String> = [(MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift")]
                .iter()
                .filter(|(bit, _)| self.held & bit != 0)
                .map(|(_, name)| (*name).to_string())
                .collect();
            keys.push("…".into());
            keys
        } else {
            self.shown.label().split('+').map(String::from).collect()
        };

        let widths: Vec<f32> = keys.iter().map(|k| (p.measure(k, &self.fonts.key) + 16.0).max(26.0)).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0 * (keys.len() as f32 - 1.0);
        let mut x = r.x + ((r.w - total) / 2.0).max(6.0);
        let y = r.y + (r.h - 26.0) / 2.0;
        p.clipped(r.inset(2.0, 2.0), || {
            for (key, width) in keys.iter().zip(&widths) {
                let chip = Rect::new(x, y, *width, 26.0);
                if key == "…" {
                    p.text(key, &self.fonts.key, chip, theme::TEXT_2, Align::Center);
                } else {
                    p.fill(chip, 4.0, theme::KEYCAP);
                    p.stroke(chip, 4.0, theme::KEYCAP_BORDER, 1.0);
                    p.text(key, &self.fonts.key, chip, theme::TEXT, Align::Center);
                }
                x += width + 6.0;
            }
        });
    }

    fn draw_slider(&self, p: &Painter, r: Rect, value: u8, widget: Widget) {
        let cy = r.y + r.h / 2.0;
        let (x0, x1) = (r.x + 10.0, r.right() - 10.0);
        let thumb_x = x0 + (x1 - x0) * f32::from(value) / 100.0;
        p.fill(Rect::new(x0, cy - 2.0, x1 - x0, 4.0), 2.0, theme::TRACK);
        p.fill(Rect::new(x0, cy - 2.0, thumb_x - x0, 4.0), 2.0, theme::ACCENT);
        p.circle(thumb_x, cy, 10.0, theme::THUMB_RING);
        p.circle(thumb_x, cy, 9.0, theme::THUMB);
        let inner = if self.dragging == Some(widget) {
            5.0
        } else if self.hover == Some(widget) {
            7.0
        } else {
            6.0
        };
        p.circle(thumb_x, cy, inner, theme::ACCENT);
    }

    fn draw_segmented(&self, p: &Painter, l: &Layout) {
        let whole = l.seg_focus.union(l.seg_list);
        p.fill(whole, 6.0, theme::INPUT);
        p.stroke(whole, 6.0, theme::CONTROL_BORDER, 1.0);
        for (widget, r, label, mode) in
            [(ModeFocus, l.seg_focus, "App em foco", Mode::Focus), (ModeList, l.seg_list, "Lista de jogos", Mode::List)]
        {
            let selected = self.mode == mode;
            let inner = r.inset(3.0, 3.0);
            if selected {
                p.fill(inner, 4.0, theme::CONTROL);
                p.fill(Rect::new(inner.x + inner.w / 2.0 - 8.0, inner.bottom() - 3.0, 16.0, 3.0), 1.5, theme::ACCENT);
            } else if self.hover == Some(widget) {
                p.fill(inner, 4.0, theme::SUBTLE_HOVER);
            }
            let color = if selected { theme::TEXT } else { theme::TEXT_2 };
            // Texto um pouco acima do centro, deixando espaço para o traço azul embaixo.
            p.text(label, &self.fonts.body, Rect::new(r.x, r.y, r.w, r.h - 5.0), color, Align::Center);
        }
    }

    fn draw_list(&self, p: &Painter, r: Rect) {
        p.fill(r, 6.0, theme::INPUT);
        p.stroke(r, 6.0, theme::INPUT_BORDER, 1.0);
        if self.games.is_empty() {
            let text = "Nenhum jogo na lista ainda.";
            p.text(text, &self.fonts.caption, r, theme::TEXT_3, Align::Center);
            return;
        }
        let overflow = self.games.len() > LIST_ROWS;
        let row_w = r.w - 8.0 - if overflow { 8.0 } else { 0.0 };
        p.clipped(r.inset(1.0, 1.0), || {
            for (slot, index) in (self.scroll..self.games.len()).take(LIST_ROWS).enumerate() {
                let row = Rect::new(r.x + 4.0, r.y + slot as f32 * ROW_H + 2.0, row_w, ROW_H - 4.0);
                if self.selected == Some(index) {
                    p.fill(row, 4.0, theme::ROW_SELECTED);
                    p.fill(Rect::new(row.x, row.y + 7.0, 3.0, row.h - 14.0), 1.5, theme::ACCENT);
                } else if self.hover_row == Some(index) {
                    p.fill(row, 4.0, theme::ROW_HOVER);
                }
                let text = Rect::new(row.x + 14.0, row.y, row.w - 20.0, row.h);
                p.text(&self.games[index], &self.fonts.body, text, theme::TEXT, Align::Left);
            }
            if overflow {
                let track_h = r.h - 8.0;
                let thumb_h = (track_h * LIST_ROWS as f32 / self.games.len() as f32).max(16.0);
                let max_scroll = (self.games.len() - LIST_ROWS) as f32;
                let thumb_y = r.y + 4.0 + (track_h - thumb_h) * self.scroll as f32 / max_scroll;
                p.fill(Rect::new(r.right() - 8.0, thumb_y, 3.0, thumb_h), 1.5, theme::SCROLL);
            }
        });
    }

    fn draw_button(&self, p: &Painter, r: Rect, icon: Option<&str>, label: &str, widget: Widget, primary: bool) {
        let enabled = self.enabled(widget);
        let hover = enabled && self.hover == Some(widget);
        let pressed = hover && self.pressed == Some(widget);
        let (background, foreground, border) = if !enabled {
            (if primary { theme::CONTROL } else { theme::CONTROL_DISABLED }, theme::TEXT_DISABLED, None)
        } else if primary {
            let background = if pressed {
                theme::ACCENT_PRESSED
            } else if hover {
                theme::ACCENT_HOVER
            } else {
                theme::ACCENT
            };
            (background, theme::ON_ACCENT, None)
        } else {
            let background = if pressed {
                theme::CONTROL_PRESSED
            } else if hover {
                theme::CONTROL_HOVER
            } else {
                theme::CONTROL
            };
            (background, if pressed { theme::TEXT_2 } else { theme::TEXT }, Some(theme::CONTROL_BORDER))
        };
        p.fill(r, 4.0, background);
        if let Some(border) = border {
            p.stroke(r, 4.0, border, 1.0);
        }
        let text_w = p.measure(label, &self.fonts.body);
        let icon_w = if icon.is_some() { 24.0 } else { 0.0 };
        let start = r.x + (r.w - text_w - icon_w) / 2.0;
        if let Some(icon) = icon {
            p.text(icon, &self.fonts.icon, Rect::new(start, r.y, 16.0, r.h), foreground, Align::Center);
        }
        p.text(label, &self.fonts.body, Rect::new(start + icon_w, r.y, text_w + 4.0, r.h), foreground, Align::Left);
    }

    fn draw_toggle(&self, p: &Painter, r: Rect) {
        let hover = self.hover == Some(Toggle);
        let cy = r.y + r.h / 2.0;
        if self.autostart {
            p.fill(r, r.h / 2.0, if hover { theme::ACCENT_HOVER } else { theme::ACCENT });
            p.circle(r.right() - r.h / 2.0, cy, if hover { 7.0 } else { 6.0 }, theme::ON_ACCENT);
        } else {
            if hover {
                p.fill(r, r.h / 2.0, theme::SUBTLE_HOVER);
            }
            p.stroke(r, r.h / 2.0, theme::TEXT_2, 1.0);
            p.circle(r.x + r.h / 2.0, cy, if hover { 6.0 } else { 5.0 }, theme::TEXT_2);
        }
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        for icon in self.icons {
            if !icon.is_invalid() {
                unsafe {
                    let _ = DestroyIcon(icon);
                }
            }
        }
    }
}

/// Ctrl/Alt/Shift apertados agora (a tecla Win não pode ser capturada: o Windows usa antes).
fn current_modifiers() -> u32 {
    let down = |vk: u32| unsafe { GetKeyState(vk as i32) } < 0;
    let mut modifiers = 0;
    if down(VK_CONTROL) {
        modifiers |= MOD_CONTROL;
    }
    if down(VK_MENU) {
        modifiers |= MOD_ALT;
    }
    if down(VK_SHIFT) {
        modifiers |= MOD_SHIFT;
    }
    modifiers
}

/// Registra e solta a combinação na hora, só para saber se outro programa já usa.
fn test_hotkey(hwnd: HWND, hotkey: Hotkey) -> std::result::Result<(), String> {
    let modifiers = HOT_KEY_MODIFIERS(hotkey.modifiers) | MOD_NOREPEAT;
    match unsafe { RegisterHotKey(Some(hwnd), TEST_HOTKEY_ID, modifiers, hotkey.vk) } {
        Ok(()) => {
            unsafe {
                let _ = UnregisterHotKey(Some(hwnd), TEST_HOTKEY_ID);
            }
            Ok(())
        }
        Err(e) if e.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() => {
            Err(format!("{} já está em uso por outro programa", hotkey.label()))
        }
        Err(e) => Err(format!("O Windows recusou essa combinação: {}", e.message())),
    }
}

/// Menu com os programas que estão com áudio aberto agora (o que o Mixer mostra).
fn playing_menu(hwnd: HWND, button: Rect) {
    let Some((games, dpi)) = with_ui(|ui| (ui.games.clone(), ui.dpi)) else { return };
    let mut names = playing_exes();
    names.retain(|name| !games.iter().any(|g| same_exe(g, name)));

    let scale = dpi as f32 / 96.0;
    let mut point = POINT { x: (button.x * scale).round() as i32, y: ((button.bottom() + 4.0) * scale).round() as i32 };
    // SAFETY: menu criado, mostrado e destruído aqui, sem o estado da janela emprestado.
    let command = unsafe {
        let _ = ClientToScreen(hwnd, &mut point);
        let Ok(menu) = CreatePopupMenu() else { return };
        if names.is_empty() {
            let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, w!("Nenhum outro programa com áudio aberto agora"));
        }
        for (i, name) in names.iter().enumerate() {
            let _ = AppendMenuW(menu, MF_STRING, i + 1, &HSTRING::from(name.replace('&', "&&")));
        }
        let command =
            TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, point.x, point.y, None, hwnd, None).0;
        let _ = DestroyMenu(menu);
        command
    };
    if command > 0
        && let Some(name) = names.get(command as usize - 1)
    {
        with_ui(|ui| ui.add_game(name));
    }
}

/// Nomes de `.exe` com sessão de áudio agora, sem repetição, em ordem alfabética.
fn playing_exes() -> Vec<String> {
    let (Ok(procs), Ok(sessions)) = (ProcessTable::snapshot(), audio::list_sessions()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    for session in &sessions {
        if let Some(exe) = procs.exe(session.pid)
            && !names.iter().any(|n| same_exe(n, exe))
        {
            names.push(exe.to_owned());
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// "Procurar .exe": escolher o executável do jogo no disco.
fn browse(hwnd: HWND) {
    let filter: Vec<u16> = "Programas (*.exe)\0*.exe\0\0".encode_utf16().collect();
    let mut file = [0u16; 1024];
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: w!("Escolha o .exe do jogo"),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY | OFN_EXPLORER,
        ..Default::default()
    };
    // SAFETY: os buffers vivem até o fim da chamada.
    if unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        let path = String::from_utf16_lossy(&file[..len]);
        with_ui(|ui| ui.add_game(&path));
    }
}

/// Ícones da janela (barra de título e Alt+Tab), na escala da tela.
fn set_icons(hwnd: HWND, dpi: u32) -> [HICON; 2] {
    let make = |metric| {
        let size = unsafe { GetSystemMetricsForDpi(metric, dpi) }.max(16) as u32;
        icons::create_hicon(IconState::Normal, size).unwrap_or_default()
    };
    let (small, big) = (make(SM_CXSMICON), make(SM_CXICON));
    unsafe {
        SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(small.0 as isize)));
        SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(big.0 as isize)));
    }
    [small, big]
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_vai_de_0_a_100() {
        let r = Rect::new(100.0, 0.0, 220.0, 24.0); // trilho útil de 110 a 310
        assert_eq!(slider_value(r, 50.0), 0);
        assert_eq!(slider_value(r, 110.0), 0);
        assert_eq!(slider_value(r, 210.0), 50);
        assert_eq!(slider_value(r, 310.0), 100);
        assert_eq!(slider_value(r, 999.0), 100);
    }

    #[test]
    fn minimo_e_maximo_se_empurram() {
        assert_eq!(adjust_range(5, 100, true, 30), (30, 100));
        assert_eq!(adjust_range(5, 40, true, 60), (60, 61));
        assert_eq!(adjust_range(5, 100, true, 100), (99, 100));
        assert_eq!(adjust_range(50, 100, false, 30), (29, 30));
        assert_eq!(adjust_range(0, 100, false, 0), (0, 1));
    }

    #[test]
    fn lista_so_aparece_no_modo_lista() {
        let focus = layout(false, false);
        let list = layout(true, false);
        assert!(focus.list.is_none() && focus.add.is_none());
        assert!(list.list.is_some() && list.remove.is_some());
        assert!(list.height > focus.height);
    }

    #[test]
    fn nada_sai_da_janela_nem_se_sobrepoe() {
        for (list_mode, save_error) in [(false, false), (true, false), (true, true)] {
            let l = layout(list_mode, save_error);
            let rects: Vec<(Widget, Rect)> =
                ALL_WIDGETS.iter().filter_map(|&w| l.rect(w).map(|r| (w, r))).collect();
            for (w, r) in &rects {
                assert!(r.x >= 0.0 && r.right() <= WIDTH && r.y >= 0.0 && r.bottom() <= l.height, "{w:?} fora");
            }
            for (i, (a, ra)) in rects.iter().enumerate() {
                for (b, rb) in &rects[i + 1..] {
                    let overlap = ra.x < rb.right() && rb.x < ra.right() && ra.y < rb.bottom() && rb.y < ra.bottom();
                    assert!(!overlap, "{a:?} e {b:?} se sobrepõem");
                }
            }
        }
    }
}
