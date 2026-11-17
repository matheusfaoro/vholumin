//! A bind: conversão entre texto ("Ctrl+Alt+M") e modificadores + código de tecla do Windows.

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

const VK_F12: u32 = 0x7B;

/// Teclas com nome (código virtual do Windows).
const NAMED_KEYS: &[(&str, u32)] = &[
    ("Pause", 0x13),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    /// Combinação de `MOD_*`.
    pub modifiers: u32,
    /// Código virtual da tecla principal.
    pub vk: u32,
}

impl Hotkey {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Err("atalho vazio".into());
        }
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last().expect("split sempre devolve ao menos uma parte");

        let mut modifiers = 0;
        for name in mods {
            modifiers |= match modifier_bit(name) {
                Some(bit) => bit,
                None if name.is_empty() => return Err("atalho mal formado".into()),
                None => return Err(format!("modificador desconhecido: {name}")),
            };
        }

        if key.is_empty() || modifier_bit(key).is_some() {
            return Err("falta a tecla principal".into());
        }
        let vk = key_code(key).ok_or_else(|| format!("tecla desconhecida: {key}"))?;
        if vk == VK_F12 {
            // Documentação do RegisterHotKey: F12 é reservada para o depurador do Windows; o
            // registro "funciona", mas o atalho nunca dispara.
            return Err("F12 é reservada pelo Windows e não funciona como atalho".into());
        }

        let hotkey = Self { modifiers, vk };
        if hotkey.needs_strong_modifier() && modifiers & (MOD_CONTROL | MOD_ALT | MOD_WIN) == 0 {
            return Err("letras, números e Espaço precisam de Ctrl, Alt ou Win junto".into());
        }
        Ok(hotkey)
    }

    /// Texto da bind, sempre na ordem Ctrl+Alt+Shift+Win+Tecla.
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = [(MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift"), (MOD_WIN, "Win")]
            .iter()
            .filter(|(bit, _)| self.modifiers & bit != 0)
            .map(|(_, name)| (*name).to_string())
            .collect();
        parts.push(key_name(self.vk));
        parts.join("+")
    }

    /// Teclas de digitação: sozinhas (ou só com Shift) atrapalhariam a escrita no Windows inteiro.
    fn needs_strong_modifier(&self) -> bool {
        matches!(self.vk, 0x20 | 0x30..=0x39 | 0x41..=0x5A)
    }
}

fn modifier_bit(name: &str) -> Option<u32> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(MOD_CONTROL),
        "alt" => Some(MOD_ALT),
        "shift" => Some(MOD_SHIFT),
        "win" | "windows" => Some(MOD_WIN),
        _ => None,
    }
}

fn key_code(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    if let [c] = upper.as_bytes()
        && (c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Some(u32::from(*c));
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x6F + n);
    }
    if let Some(n) = upper.strip_prefix("NUM").and_then(|n| n.parse::<u32>().ok())
        && upper.len() == 4
    {
        return Some(0x60 + n);
    }
    NAMED_KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, vk)| vk)
}

fn key_name(vk: u32) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => NAMED_KEYS
            .iter()
            .find(|&&(_, v)| v == vk)
            .map_or_else(|| format!("0x{vk:02X}"), |&(n, _)| n.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_padrao() {
        let h = Hotkey::parse("Ctrl+Alt+M").unwrap();
        assert_eq!(h, Hotkey { modifiers: MOD_CONTROL | MOD_ALT, vk: 0x4D });
        assert_eq!(h.label(), "Ctrl+Alt+M");
    }

    #[test]
    fn ignora_espacos_e_maiusculas() {
        assert_eq!(Hotkey::parse(" alt + CTRL + m ").unwrap(), Hotkey::parse("Ctrl+Alt+M").unwrap());
    }

    #[test]
    fn texto_volta_igual() {
        for text in ["Ctrl+Alt+M", "Shift+F13", "F13", "Win+Num5", "Ctrl+Shift+PageDown", "Alt+0", "Ctrl+Space", "Pause"] {
            assert_eq!(Hotkey::parse(text).unwrap().label(), text);
        }
    }

    #[test]
    fn teclas_especiais() {
        assert_eq!(Hotkey::parse("F24").unwrap().vk, 0x87);
        assert_eq!(Hotkey::parse("Ctrl+Num0").unwrap().vk, 0x60);
        assert_eq!(Hotkey::parse("Alt+Down").unwrap().vk, 0x28);
    }

    #[test]
    fn letra_sozinha_ou_so_com_shift_nao_vale() {
        assert!(Hotkey::parse("M").is_err());
        assert!(Hotkey::parse("Shift+M").is_err());
        assert!(Hotkey::parse("Shift+Space").is_err());
        assert!(Hotkey::parse("Win+M").is_ok());
    }

    #[test]
    fn erros_claros() {
        assert_eq!(Hotkey::parse("").unwrap_err(), "atalho vazio");
        assert_eq!(Hotkey::parse("Ctrl+Alt").unwrap_err(), "falta a tecla principal");
        assert_eq!(Hotkey::parse("Ctrl+").unwrap_err(), "falta a tecla principal");
        assert_eq!(Hotkey::parse("Ctrl++M").unwrap_err(), "atalho mal formado");
        assert_eq!(Hotkey::parse("Hyper+M").unwrap_err(), "modificador desconhecido: Hyper");
        assert_eq!(Hotkey::parse("Ctrl+Foo").unwrap_err(), "tecla desconhecida: Foo");
        assert!(Hotkey::parse("Ctrl+F25").is_err());
        assert_eq!(Hotkey::parse("Ctrl+Alt+Shift+F12").unwrap_err(), "F12 é reservada pelo Windows e não funciona como atalho");
        assert!(Hotkey::parse("Ctrl+Num10").is_err());
    }
}
