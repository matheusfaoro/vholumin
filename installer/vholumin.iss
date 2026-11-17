; Instalador do vholumin (Inno Setup 6.7 ou mais novo).
;
; Gerar:
;   cargo build --release
;   ISCC installer\vholumin.iss
; Sai em: target\instalador\vholumin-setup-<versão>.exe
;
; Instala só para o usuário atual (sem pedir administrador), em
; %LOCALAPPDATA%\Programs\vholumin. A versão vem do próprio vholumin.exe.

#define AppExe AddBackslash(SourcePath) + "..\target\release\vholumin.exe"
#define AppVersion GetStringFileInfo(AppExe, "ProductVersion")

[Setup]
; Identificador fixo: é por ele que o Windows reconhece atualizações e a desinstalação.
AppId={{AB189535-6B3B-4BC5-A44D-D350663EE364}
AppName=vholumin
AppVersion={#AppVersion}
AppVerName=vholumin {#AppVersion}
AppPublisher=Matheus Faoro
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Instalador do vholumin
PrivilegesRequired=lowest
DefaultDirName={autopf}\vholumin
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\target\instalador
OutputBaseFilename=vholumin-setup-{#AppVersion}
SetupIconFile=..\res\vholumin.ico
UninstallDisplayIcon={app}\vholumin.exe
UninstallDisplayName=vholumin
; Assistente escuro, combinando com a janela de configurações do vholumin.
WizardStyle=modern dark windows11
; Ícone do canto já pintado sobre o fundo do assistente escuro (#2B2B2B); com transparência
; o Inno desenha um quadrado branco atrás. Gerado por: cargo test exportar_imagens_do_app -- --ignored
WizardSmallImageFile=..\res\instalador-55.bmp,..\res\instalador-110.bmp
Compression=lzma2/max
SolidCompression=yes
ShowLanguageDialog=no

[Languages]
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "autostart"; Description: "Iniciar o vholumin junto com o Windows"

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\vholumin"; Filename: "{app}\vholumin.exe"

[Run]
; A opção "Iniciar com o Windows" é gravada pelo próprio vholumin (config.toml + registro),
; com a mesma lógica do menu e da janela de configurações.
Filename: "{app}\vholumin.exe"; Parameters: "--iniciar-com-windows sim"; Tasks: autostart; Flags: runhidden waituntilterminated
Filename: "{app}\vholumin.exe"; Parameters: "--iniciar-com-windows nao"; Tasks: not autostart; Flags: runhidden waituntilterminated
Filename: "{app}\vholumin.exe"; Description: "Abrir o vholumin agora"; Flags: postinstall nowait skipifsilent

[Code]
const
  WM_CLOSE = $0010;
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

{ Pede para a janela (se existir) fechar. }
procedure CloseWindow(const ClassName: String);
var
  Wnd: HWND;
begin
  Wnd := FindWindowByClassName(ClassName);
  if Wnd <> 0 then
    PostMessage(Wnd, WM_CLOSE, 0, 0);
end;

{ Fecha o vholumin (a cópia da bandeja e a janela de configurações) antes de mexer nos
  arquivos, e espera ele sair. }
procedure CloseRunningApp;
var
  Tries: Integer;
begin
  CloseWindow('vholumin.configuracoes');
  CloseWindow('vholumin.janela');
  Tries := 0;
  while ((FindWindowByClassName('vholumin.janela') <> 0) or
         (FindWindowByClassName('vholumin.configuracoes') <> 0)) and (Tries < 50) do
  begin
    Sleep(100);
    Tries := Tries + 1;
  end;
  { O processo termina logo depois de a janela sumir. }
  Sleep(400);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseRunningApp;
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep = usUninstall then
    CloseRunningApp;

  if CurUninstallStep = usPostUninstall then
  begin
    { Tira o "iniciar com o Windows", se ele aponta para esta instalação. }
    if RegQueryStringValue(HKCU, RunKey, 'vholumin', Command) and
       (Pos(Lowercase(ExpandConstant('{app}')), Lowercase(Command)) > 0) then
      RegDeleteValue(HKCU, RunKey, 'vholumin');

    { As configurações ficam, a não ser que o usuário peça para apagar. }
    if not UninstallSilent and
       (MsgBox('Apagar também as configurações e o log do vholumin?', mbConfirmation,
               MB_YESNO or MB_DEFBUTTON2) = IDYES) then
      DelTree(ExpandConstant('{userappdata}\vholumin'), True, True, True);
  end;
end;
