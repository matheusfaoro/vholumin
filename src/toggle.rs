//! A ação da bind: descobre o alvo e alterna o volume dele.

use crate::audio::{self, Session};
use crate::processes::ProcessTable;
use crate::target::{self, MatchKind, Toggle};

/// Volumes mínimo e máximo (0.0 a 1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeRange {
    pub min: f32,
    pub max: f32,
}

/// Quem a bind controla.
pub enum Target<'a> {
    /// O processo dono da janela em foco (PID).
    Focus(u32),
    /// Os jogos da lista (nomes de `.exe`).
    List(&'a [String]),
}

/// Uma sessão alterada pela bind.
pub struct Change {
    pub exe: String,
    pub before: f32,
    pub ok: bool,
    pub session: Session,
}

/// O que a bind fez.
pub struct Report {
    pub kind: MatchKind,
    pub toggle: Toggle,
    pub level: f32,
    pub changes: Vec<Change>,
}

impl Report {
    /// Nomes dos programas alterados, sem repetição ("cs2.exe" ou "a.exe, b.exe").
    pub fn target_names(&self) -> String {
        let mut names: Vec<&str> = Vec::new();
        for c in &self.changes {
            if !names.iter().any(|n| target::same_exe(n, &c.exe)) {
                names.push(&c.exe);
            }
        }
        names.join(", ")
    }
}

pub enum Outcome {
    Changed(Report),
    /// Nada para alternar; o texto explica o porquê (vai para o ícone).
    NotFound(String),
}

/// Volume em porcentagem inteira, como o Mixer mostra.
pub fn pct(volume: f32) -> u32 {
    (volume * 100.0).round() as u32
}

/// Alterna o volume do alvo. Precisa do COM inicializado na thread.
pub fn toggle(target: Target, range: VolumeRange) -> windows::core::Result<Outcome> {
    let procs = ProcessTable::snapshot()?;
    let sessions = audio::list_sessions()?;
    let pids: Vec<u32> = sessions.iter().map(|s| s.pid).collect();

    let (selection, context, not_found) = match target {
        Target::Focus(fg_pid) => {
            let exe = procs.exe(fg_pid).unwrap_or("?");
            (
                target::select_for_focus(fg_pid, &pids, &procs),
                format!("foco em {exe} (PID {fg_pid})"),
                format!("nenhum áudio encontrado em {exe}"),
            )
        }
        Target::List(games) => (
            target::select_from_list(&pids, &procs, games),
            format!("modo lista ({} jogo(s))", games.len()),
            if games.is_empty() {
                "a lista de jogos está vazia".to_string()
            } else {
                "nenhum jogo da lista está com áudio aberto".to_string()
            },
        ),
    };

    let Some(selection) = selection else {
        logf!("bind: {context} — nenhum áudio encontrado ({} sessões no sistema)", sessions.len());
        return Ok(Outcome::NotFound(not_found));
    };

    let volumes: Vec<f32> = selection.indices.iter().map(|&i| sessions[i].volume).collect();
    let toggle = target::decide(&volumes, range.min);
    let level = match toggle {
        Toggle::ToMin => range.min,
        Toggle::ToMax => range.max,
    };

    let changes: Vec<Change> = sessions
        .into_iter()
        .enumerate()
        .filter(|(i, _)| selection.indices.contains(i))
        .map(|(_, session)| {
            let result = session.set_volume(level);
            if let Err(e) = &result {
                logf!("  falha ao mudar o volume do PID {}: {e}", session.pid);
            }
            Change {
                exe: procs.exe(session.pid).unwrap_or("?").to_owned(),
                before: session.volume,
                ok: result.is_ok(),
                session,
            }
        })
        .collect();

    let report = Report { kind: selection.kind, toggle, level, changes };
    logf!(
        "bind: {context} — alvo {} por {:?} — {} sessão(ões) → {}%",
        report.target_names(),
        report.kind,
        report.changes.len(),
        pct(level)
    );
    for c in &report.changes {
        logf!(
            "  {} (PID {}) em '{}': {}% → {}%{}",
            c.exe,
            c.session.pid,
            c.session.device,
            pct(c.before),
            pct(level),
            if c.ok { "" } else { " (FALHOU)" }
        );
    }
    Ok(Outcome::Changed(report))
}
