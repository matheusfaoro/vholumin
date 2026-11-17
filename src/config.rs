//! Configurações em `%APPDATA%\vholumin\config.toml`.
//!
//! Problemas no arquivo nunca impedem o programa de rodar: o que estiver errado volta ao
//! padrão e o problema é registrado (no log e no menu do ícone).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::Deserialize;

use crate::hotkey::Hotkey;
use crate::target::same_exe;
use crate::toggle::VolumeRange;

pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+M";
const DEFAULT_MIN: u8 = 5;
const DEFAULT_MAX: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// O app em foco na hora da bind.
    Focus,
    /// Os jogos da lista, mesmo fora de foco.
    List,
}

impl Mode {
    pub fn key(self) -> &'static str {
        match self {
            Mode::Focus => "foco",
            Mode::List => "lista",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub hotkey: Hotkey,
    pub min_pct: u8,
    pub max_pct: u8,
    pub mode: Mode,
    /// Nomes de `.exe` (sem repetição, sempre terminando em `.exe`).
    pub games: Vec<String>,
    pub start_with_windows: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: Hotkey::parse(DEFAULT_HOTKEY).expect("bind padrão válida"),
            min_pct: DEFAULT_MIN,
            max_pct: DEFAULT_MAX,
            mode: Mode::Focus,
            games: Vec::new(),
            // O instalador (Fase 4) liga isso. Ligar por padrão faria a versão de
            // desenvolvimento abrir sozinha com o Windows.
            start_with_windows: false,
        }
    }
}

/// O resultado da leitura: a configuração em uso e os problemas encontrados no arquivo.
pub struct Loaded {
    pub config: Config,
    pub problems: Vec<String>,
}

impl Config {
    pub fn range(&self) -> VolumeRange {
        VolumeRange { min: f32::from(self.min_pct) / 100.0, max: f32::from(self.max_pct) / 100.0 }
    }

    pub fn path() -> Option<PathBuf> {
        crate::log::data_dir().map(|dir| dir.join("config.toml"))
    }

    /// Lê o arquivo. Se ele não existir, cria com os padrões.
    pub fn load() -> Loaded {
        let Some(path) = Self::path() else {
            return Loaded { config: Self::default(), problems: vec!["pasta %APPDATA% não encontrada".into()] };
        };
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                let problems = match config.save() {
                    Ok(()) => Vec::new(),
                    Err(e) => vec![format!("não foi possível criar o config.toml: {e}")],
                };
                Loaded { config, problems }
            }
            Err(e) => Loaded {
                config: Self::default(),
                problems: vec![format!("não foi possível ler o config.toml: {e}")],
            },
        }
    }

    pub fn parse(text: &str) -> Loaded {
        // O Bloco de Notas pode salvar com BOM (marca invisível no início), que o TOML não aceita.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let raw: Raw = match toml::from_str(text) {
            Ok(raw) => raw,
            Err(e) => {
                let line = e.span().map(|s| text[..s.start.min(text.len())].lines().count().max(1));
                let place = line.map(|l| format!(" (linha {l})")).unwrap_or_default();
                return Loaded {
                    config: Self::default(),
                    problems: vec![format!("config.toml inválido{place}: {} — usando os padrões", e.message().trim())],
                };
            }
        };

        let mut problems: Vec<String> = raw.unknown.keys().map(|k| format!("opção desconhecida ignorada: {k}")).collect();

        let hotkey = Hotkey::parse(&raw.atalho).unwrap_or_else(|e| {
            problems.push(format!("atalho \"{}\": {e} — usando {DEFAULT_HOTKEY}", raw.atalho));
            Hotkey::parse(DEFAULT_HOTKEY).expect("bind padrão válida")
        });

        let (min_pct, max_pct) = match (u8::try_from(raw.volume_minimo), u8::try_from(raw.volume_maximo)) {
            (Ok(min), Ok(max)) if min < max && max <= 100 => (min, max),
            _ => {
                problems.push(format!(
                    "volumes {} e {} inválidos (0 a 100, mínimo menor que o máximo) — usando {DEFAULT_MIN} e {DEFAULT_MAX}",
                    raw.volume_minimo, raw.volume_maximo
                ));
                (DEFAULT_MIN, DEFAULT_MAX)
            }
        };

        let mode = match raw.modo.trim().to_lowercase().as_str() {
            "foco" => Mode::Focus,
            "lista" => Mode::List,
            other => {
                problems.push(format!("modo \"{other}\" desconhecido (use \"foco\" ou \"lista\") — usando \"foco\""));
                Mode::Focus
            }
        };

        let config = Config {
            hotkey,
            min_pct,
            max_pct,
            mode,
            games: normalize_games(&raw.jogos),
            start_with_windows: raw.iniciar_com_windows,
        };
        Loaded { config, problems }
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "pasta %APPDATA% não encontrada"))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.render())
    }

    /// O arquivo completo, com comentários explicando cada opção.
    pub fn render(&self) -> String {
        let games = self.games.iter().map(|g| toml_string(g)).collect::<Vec<_>>().join(", ");
        format!(
            r#"# vholumin — configurações
#
# Este arquivo é lido ao abrir o vholumin e ao escolher "Recarregar configurações" no
# menu do ícone da bandeja. Linhas que começam com # são comentários.

# Atalho que alterna o volume. Exemplos: "Ctrl+Alt+M", "Ctrl+Shift+F13", "F13".
# Letras, números e Espaço precisam de Ctrl, Alt ou Win junto.
atalho = {atalho}

# Volumes do jogo, em porcentagem (0 a 100). O mínimo tem que ser menor que o máximo.
volume_minimo = {min}
volume_maximo = {max}

# Qual programa o atalho controla:
#   "foco"  = o programa que está em foco na hora do atalho
#   "lista" = os jogos da lista abaixo, mesmo fora de foco
modo = "{modo}"

# Jogos do modo "lista" (nome do .exe). Exemplo: ["VALORANT-Win64-Shipping.exe", "cs2.exe"]
jogos = [{games}]

# Abrir o vholumin junto com o Windows.
iniciar_com_windows = {start}
"#,
            atalho = toml_string(&self.hotkey.label()),
            min = self.min_pct,
            max = self.max_pct,
            modo = self.mode.key(),
            start = self.start_with_windows,
        )
    }
}

/// Campos como estão no arquivo, antes da validação.
#[derive(Deserialize)]
#[serde(default)]
struct Raw {
    atalho: String,
    volume_minimo: i64,
    volume_maximo: i64,
    modo: String,
    jogos: Vec<String>,
    iniciar_com_windows: bool,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

impl Default for Raw {
    fn default() -> Self {
        let d = Config::default();
        Self {
            atalho: d.hotkey.label(),
            volume_minimo: d.min_pct.into(),
            volume_maximo: d.max_pct.into(),
            modo: d.mode.key().into(),
            jogos: d.games,
            iniciar_com_windows: d.start_with_windows,
            unknown: BTreeMap::new(),
        }
    }
}

/// Aceita "cs2", "cs2.exe" ou o caminho completo; devolve só "cs2.exe".
pub fn normalize_game(entry: &str) -> Option<String> {
    let entry = entry.trim();
    let file = entry.rsplit(['\\', '/']).next().unwrap_or(entry).trim();
    if file.is_empty() {
        return None;
    }
    Some(if file.to_lowercase().ends_with(".exe") { file.to_string() } else { format!("{file}.exe") })
}

/// Normaliza a lista inteira, sem repetição.
fn normalize_games(raw: &[String]) -> Vec<String> {
    let mut games: Vec<String> = Vec::new();
    for file in raw.iter().filter_map(|entry| normalize_game(entry)) {
        if !games.iter().any(|g| same_exe(g, &file)) {
            games.push(file);
        }
    }
    games
}

fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padrao_vai_e_volta_sem_problemas() {
        let loaded = Config::parse(&Config::default().render());
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn valores_personalizados_vao_e_voltam() {
        let config = Config {
            hotkey: Hotkey::parse("Ctrl+Shift+F13").unwrap(),
            min_pct: 0,
            max_pct: 80,
            mode: Mode::List,
            games: vec!["VALORANT-Win64-Shipping.exe".into(), "cs2.exe".into()],
            start_with_windows: true,
        };
        let loaded = Config::parse(&config.render());
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.config, config);
    }

    #[test]
    fn arquivo_vazio_usa_padroes() {
        let loaded = Config::parse("");
        assert!(loaded.problems.is_empty());
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn atalho_invalido_volta_ao_padrao() {
        let loaded = Config::parse("atalho = \"Shift+M\"\nvolume_minimo = 10");
        assert_eq!(loaded.config.hotkey.label(), DEFAULT_HOTKEY);
        assert_eq!(loaded.config.min_pct, 10);
        assert_eq!(loaded.problems.len(), 1);
        assert!(loaded.problems[0].contains("Shift+M"));
    }

    #[test]
    fn volumes_invalidos_voltam_ao_padrao() {
        for text in [
            "volume_minimo = 50\nvolume_maximo = 50",
            "volume_minimo = 60\nvolume_maximo = 40",
            "volume_minimo = -1",
            "volume_maximo = 101",
        ] {
            let loaded = Config::parse(text);
            assert_eq!((loaded.config.min_pct, loaded.config.max_pct), (DEFAULT_MIN, DEFAULT_MAX), "{text}");
            assert_eq!(loaded.problems.len(), 1, "{text}");
        }
    }

    #[test]
    fn modo_invalido_volta_para_foco() {
        let loaded = Config::parse("modo = \"jogo\"");
        assert_eq!(loaded.config.mode, Mode::Focus);
        assert_eq!(loaded.problems.len(), 1);
        assert_eq!(Config::parse("modo = \" LISTA \"").config.mode, Mode::List);
    }

    #[test]
    fn lista_de_jogos_e_normalizada() {
        let loaded = Config::parse(r#"jogos = ["cs2", "  ", "C:\\Riot\\VALORANT-Win64-Shipping.exe", "CS2.exe", "game.EXE"]"#);
        assert_eq!(loaded.config.games, ["cs2.exe", "VALORANT-Win64-Shipping.exe", "game.EXE"]);
    }

    #[test]
    fn erro_de_sintaxe_usa_padroes_e_diz_a_linha() {
        let loaded = Config::parse("volume_minimo = 5\nvolume_maximo = = 100\n");
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.problems.len(), 1);
        assert!(loaded.problems[0].contains("linha 2"), "{}", loaded.problems[0]);
    }

    #[test]
    fn tipo_errado_usa_padroes() {
        let loaded = Config::parse("volume_minimo = \"cinco\"");
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.problems.len(), 1);
    }

    #[test]
    fn arquivo_com_bom_do_bloco_de_notas() {
        let loaded = Config::parse("\u{feff}volume_minimo = 10\n");
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.config.min_pct, 10);
    }

    #[test]
    fn opcao_desconhecida_vira_aviso() {
        let loaded = Config::parse("volume_minimio = 10");
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.problems, ["opção desconhecida ignorada: volume_minimio"]);
    }
}
