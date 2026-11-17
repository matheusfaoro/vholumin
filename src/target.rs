//! Lógica pura: qual sessão de áudio é o alvo, e para onde o volume vai.

use crate::processes::ProcessTable;

/// Diferença mínima para considerar um volume "acima do mínimo" (0,5%).
pub const TOLERANCE: f32 = 0.005;

/// Como o alvo foi encontrado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Só o próprio processo em foco.
    Focused,
    /// Inclui outros processos com o mesmo nome de `.exe` do que está em foco.
    SameExe,
    /// Jogos da lista (modo lista).
    List,
}

/// Compara nomes de `.exe` sem diferenciar maiúsculas/minúsculas, como o Windows faz.
pub fn same_exe(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Selection {
    /// Índices das sessões escolhidas.
    pub indices: Vec<usize>,
    pub kind: MatchKind,
}

/// Escolhe as sessões de áudio do app em foco.
///
/// `session_pids[i]` é o PID dono da sessão `i`. Entram as sessões do processo em foco e as
/// de processos com o mesmo nome de `.exe` (navegadores e apps tipo Discord tocam som num
/// processo "irmão" de mesmo nome).
///
/// Processos filhos com outro nome ficam de fora de propósito: a área de trabalho
/// (`explorer.exe`) e os launchers são "pais" de quase tudo que o usuário abre, e incluir
/// filhos abaixaria programas que não estão em foco.
pub fn select_for_focus(fg_pid: u32, session_pids: &[u32], procs: &ProcessTable) -> Option<Selection> {
    if fg_pid == 0 {
        return None;
    }
    let fg_exe = procs.exe(fg_pid);
    let same_app = |pid: u32| {
        pid == fg_pid || fg_exe.is_some_and(|fg| procs.exe(pid).is_some_and(|exe| same_exe(exe, fg)))
    };
    let indices = matching(session_pids, same_app);
    if indices.is_empty() {
        return None;
    }
    let kind = if indices.iter().all(|&i| session_pids[i] == fg_pid) { MatchKind::Focused } else { MatchKind::SameExe };
    Some(Selection { indices, kind })
}

/// Modo lista: as sessões de qualquer processo cujo `.exe` está na lista, em foco ou não.
pub fn select_from_list(session_pids: &[u32], procs: &ProcessTable, games: &[String]) -> Option<Selection> {
    let in_list = |pid: u32| procs.exe(pid).is_some_and(|exe| games.iter().any(|game| same_exe(game, exe)));
    let indices = matching(session_pids, in_list);
    (!indices.is_empty()).then_some(Selection { indices, kind: MatchKind::List })
}

fn matching(session_pids: &[u32], pred: impl Fn(u32) -> bool) -> Vec<usize> {
    session_pids
        .iter()
        .enumerate()
        .filter(|&(_, &pid)| pred(pid))
        .map(|(i, _)| i)
        .collect()
}

/// Para onde a bind manda o volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    ToMin,
    ToMax,
}

/// Regra de alternância: se alguma sessão está acima do mínimo, todas vão para o mínimo;
/// senão, todas vão para o máximo.
pub fn decide(volumes: &[f32], min: f32) -> Toggle {
    if volumes.iter().any(|&v| v > min + TOLERANCE) {
        Toggle::ToMin
    } else {
        Toggle::ToMax
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: f32 = 0.05;

    // PIDs: o Explorer (1) abriu o launcher, o Discord e o navegador; o launcher abriu o jogo.
    fn procs() -> ProcessTable {
        let mut t = ProcessTable::default();
        t.insert(1, "explorer.exe");
        t.insert(100, "Launcher.exe");
        t.insert(200, "game.exe");
        t.insert(300, "Game.EXE");
        t.insert(400, "discord.exe");
        t.insert(500, "chrome.exe");
        t.insert(510, "chrome.exe");
        t
    }

    #[test]
    fn foco_com_audio_proprio() {
        let s = select_for_focus(400, &[200, 400, 510], &procs()).unwrap();
        assert_eq!(s, Selection { indices: vec![1], kind: MatchKind::Focused });
    }

    #[test]
    fn navegador_toca_som_em_outro_processo_de_mesmo_nome() {
        // Janela do navegador em foco (500), som sai de outro chrome.exe (510).
        let s = select_for_focus(500, &[400, 510], &procs()).unwrap();
        assert_eq!(s, Selection { indices: vec![1], kind: MatchKind::SameExe });
    }

    #[test]
    fn mesmo_nome_sem_diferenciar_maiusculas() {
        let s = select_for_focus(200, &[200, 300, 400], &procs()).unwrap();
        assert_eq!(s, Selection { indices: vec![0, 1], kind: MatchKind::SameExe });
    }

    #[test]
    fn area_de_trabalho_em_foco_nao_mexe_nos_programas_abertos() {
        // Bug real da Fase 1: com o Explorer em foco, Steam e navegador foram abaixados.
        assert_eq!(select_for_focus(1, &[100, 200, 400, 510], &procs()), None);
    }

    #[test]
    fn launcher_em_foco_nao_mexe_no_jogo() {
        assert_eq!(select_for_focus(100, &[200, 300], &procs()), None);
    }

    #[test]
    fn nada_encontrado() {
        assert_eq!(select_for_focus(400, &[200, 510], &procs()), None);
        assert_eq!(select_for_focus(400, &[], &procs()), None);
        assert_eq!(select_for_focus(999, &[200], &procs()), None);
    }

    fn games(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn lista_pega_jogos_fora_de_foco_sem_diferenciar_maiusculas() {
        let s = select_from_list(&[400, 200, 300, 510], &procs(), &games(&["GAME.exe"])).unwrap();
        assert_eq!(s, Selection { indices: vec![1, 2], kind: MatchKind::List });
    }

    #[test]
    fn lista_com_varios_jogos() {
        let s = select_from_list(&[400, 200, 510], &procs(), &games(&["game.exe", "chrome.exe"])).unwrap();
        assert_eq!(s.indices, vec![1, 2]);
    }

    #[test]
    fn lista_vazia_ou_sem_jogo_aberto() {
        assert_eq!(select_from_list(&[200, 400], &procs(), &[]), None);
        assert_eq!(select_from_list(&[400, 510], &procs(), &games(&["game.exe"])), None);
        assert_eq!(select_from_list(&[999], &procs(), &games(&["game.exe"])), None);
    }

    #[test]
    fn sem_janela_em_foco() {
        assert_eq!(select_for_focus(0, &[0, 200], &procs()), None);
    }

    #[test]
    fn no_maximo_vai_para_o_minimo() {
        assert_eq!(decide(&[1.0], MIN), Toggle::ToMin);
    }

    #[test]
    fn no_minimo_vai_para_o_maximo() {
        assert_eq!(decide(&[0.05], MIN), Toggle::ToMax);
        // Arredondamento do Windows não pode confundir a regra.
        assert_eq!(decide(&[0.050_001], MIN), Toggle::ToMax);
    }

    #[test]
    fn volume_manual_no_meio_vai_para_o_minimo() {
        assert_eq!(decide(&[0.6], MIN), Toggle::ToMin);
    }

    #[test]
    fn abaixo_do_minimo_vai_para_o_maximo() {
        assert_eq!(decide(&[0.02], MIN), Toggle::ToMax);
    }

    #[test]
    fn varias_sessoes_basta_uma_acima_do_minimo() {
        assert_eq!(decide(&[0.05, 0.8], MIN), Toggle::ToMin);
        assert_eq!(decide(&[0.05, 0.05], MIN), Toggle::ToMax);
    }
}
