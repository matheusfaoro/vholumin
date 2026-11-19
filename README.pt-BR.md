# vholumin

[English](README.md) · **Português**

Alterna o volume do jogo no Mixer do Windows com uma tecla de atalho.

Aperte **`Ctrl+Alt+M`** e o volume do jogo cai para o mínimo. Aperte de novo e ele volta
para o máximo. Só o jogo muda: Discord, música e navegador continuam como estão.

O vholumin fica no ícone da bandeja, não gasta processador enquanto espera e não encosta
no jogo. Ele mexe no volume pelo mesmo caminho que o Mixer do Windows usa.

## Como funciona

- **A regra:** se o jogo está acima do volume mínimo, o atalho leva para o mínimo. Se já
  está no mínimo, leva para o máximo. Os padrões são **5%** e **100%**.
- **Sempre o volume real:** o vholumin lê o volume atual no Windows a cada aperto. Mexer no
  Mixer na mão não confunde nada. Por exemplo: jogo em 60% → atalho → 5% → atalho → 100%.
- **Qual programa ele controla**, em dois modos:
  - **App em foco** (padrão): o programa que está na frente na hora do atalho, junto com
    os processos de mesmo nome. Navegadores e o Discord, por exemplo, tocam som num
    processo "irmão".
  - **Lista de jogos:** os `.exe` que você cadastrar, mesmo com o jogo em segundo plano.
    Por exemplo, você no Discord e o jogo aberto atrás. Se mais de um jogo da lista estiver
    aberto, todos alternam juntos.
- Funciona em todas as saídas de áudio ao mesmo tempo (fone, caixa, HDMI…).

## Ícone da bandeja

O ícone é o único aviso visual: não aparece janela nem toca som.

| Ícone | Quando |
|---|---|
| Azul, duas ondas | Jogo no máximo (e ao iniciar) |
| Cinza, uma onda | Jogo no mínimo |
| Vermelho, com X | Pisca por um instante quando o atalho não encontrou áudio do alvo |
| Cinza, riscado | Atalho pausado, janela de configurações aberta ou atalho já usado por outro programa |

Passe o mouse no ícone para ver o detalhe, por exemplo `vholumin — cs2.exe em 5%`.

- **Clique esquerdo:** abre as configurações.
- **Clique direito:** menu com Configurações, Modo, Pausar atalho, Iniciar com o Windows,
  Recarregar o config.toml, Abrir pasta do vholumin e Sair.

O jogo só aparece no Mixer depois de tocar algum som. Antes disso, o atalho mostra o ícone
vermelho.

## Instalação

Requer **Windows 10 ou 11, 64 bits**.

1. Baixe o `vholumin-setup-<versão>.exe` na
   [versão mais recente](https://github.com/matheusfaoro/vholumin/releases/latest) e execute.
2. Ele instala só para o seu usuário, **sem pedir administrador**, em
   `%LOCALAPPDATA%\Programs\vholumin`, com atalho no Menu Iniciar.
3. Deixe marcada a opção **"Iniciar o vholumin junto com o Windows"** se quiser que ele
   abra sozinho.

O instalador não é assinado digitalmente, então o Windows pode mostrar "O Windows protegeu
o computador". Clique em **Mais informações → Executar assim mesmo**.

**Atualizar:** rode o instalador da versão nova por cima. Ele fecha o vholumin sozinho e
mantém as configurações.

**Desinstalar:** Configurações do Windows → Aplicativos → vholumin → Desinstalar. Ele
pergunta se você quer apagar também as configurações e o log.

## Configurações

Clique no ícone da bandeja para abrir a janela de configurações:

- **Atalho:** clique no campo e aperte a combinação. Ela fica verde se estiver livre e
  vermelha se outro programa já usa.
- **Volume mínimo e máximo:** dois controles deslizantes. O mínimo é sempre menor que o
  máximo.
- **Modo:** "App em foco" ou "Lista de jogos". No modo lista, dá para adicionar o que está
  tocando agora, procurar um `.exe` ou remover um jogo.
- **Iniciar com o Windows.**

Enquanto a janela está aberta, o atalho fica pausado.

As configurações ficam em `%APPDATA%\vholumin\config.toml`. Não é preciso editar à mão,
mas o arquivo é simples:

```toml
atalho = "Ctrl+Alt+M"
volume_minimo = 5
volume_maximo = 100
modo = "foco"            # "foco" ou "lista"
jogos = ["VALORANT-Win64-Shipping.exe", "cs2.exe"]
iniciar_com_windows = true
```

Um valor errado no arquivo nunca impede o vholumin de abrir. O campo com erro volta ao
padrão, e o problema aparece no topo do menu do ícone e no log. A tecla **F12** não é
aceita no atalho, porque o Windows a reserva e ela nunca dispara.

## Anti-cheat

O vholumin foi feito para não chamar a atenção de anti-cheats. Ele **nunca**:

- abre o processo do jogo (nem só para ler o nome);
- lê ou escreve na memória de outro programa, injeta DLL ou instala hook de teclado;
- simula teclas ou cliques;
- instala driver, serviço ou overlay, nem pede administrador.

Ele usa só o registro de atalho do Windows, a janela em foco, a lista de processos e a API
de áudio do Windows, que é a mesma do Mixer.

Mesmo assim, ninguém consegue garantir 100% para todo anti-cheat. A segurança vem do
método: o vholumin faz o mesmo tipo de coisa que o Mixer do Windows faz.

## Log e diagnóstico

- **Log:** `%APPDATA%\vholumin\log.txt`, com tamanho máximo de ~1 MB. Registra o que cada
  aperto do atalho encontrou. O menu do ícone tem **Abrir pasta do vholumin**, que leva
  direto até ele.
- **Comandos** (rodar num terminal):
  - `vholumin.exe --listar`: mostra todas as sessões de áudio, como o Mixer vê.
  - `vholumin.exe --alternar <PID>`: faz o que o atalho faria se o processo `<PID>`
    estivesse em foco.

## Compilar a partir do código

Requisitos:

- [Rust](https://rustup.rs) 1.85 ou mais novo, com o toolchain MSVC
  (`stable-x86_64-pc-windows-msvc`) e as Build Tools do Visual Studio com o Windows SDK.
- [Inno Setup](https://jrsoftware.org/isinfo.php) 6.7 ou mais novo, só para gerar o
  instalador.

```bat
cargo build --release
ISCC installer\vholumin.iss
```

- O programa sai em `target\release\vholumin.exe`.
- O instalador sai em `target\instalador\vholumin-setup-<versão>.exe`.
- A versão fica só no `Cargo.toml`. O `.exe` e o instalador pegam a versão de lá.

Outros comandos:

- `cargo test`: roda os testes automáticos.
- `cargo test exportar_imagens_do_app -- --ignored`: gera de novo o ícone do app e as
  imagens do instalador em `res/`.

Para testar uma cópia de desenvolvimento sem mexer nas configurações e no log da cópia
instalada, defina a variável `VHOLUMIN_DATA_DIR` com outra pasta antes de rodar.

### Estrutura

```
src/          código do programa (bandeja, áudio, atalho, configurações, janela)
res/          manifesto e ícone embutidos no .exe, imagens do instalador
installer/    script do instalador (Inno Setup)
build.rs      embute manifesto, ícone e versão no .exe
```

## Licença

Distribuído sob a [licença MIT](LICENSE). © 2026 Matheus Faoro
