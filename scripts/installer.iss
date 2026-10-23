; LofiLoop — skrip installer Windows
;
; Dibangun oleh scripts/build-installer.ps1. Berkas ini dipakai oleh Inno Setup.
; Tujuan: satu berkas exe yang memasang aplikasi SEKALIGUS FFmpeg, supaya pengguna
; tidak perlu memasang apa pun sendiri.
;
; Versi dan jalur berkas diberikan lewat /D saat kompilasi, lihat build-installer.ps1.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\lofiloop-app.exe"
#endif
#ifndef FfmpegDir
  #define FfmpegDir ""
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
AppId={{8F3A2C71-6B4D-4E29-9A15-7C0E5D3B81A4}
AppName=LofiLoop
AppVersion={#AppVersion}
AppPublisher=Samuel Indra Bastian
DefaultDirName={autopf}\LofiLoop
DefaultGroupName=LofiLoop
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=LofiLoop-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UninstallDisplayIcon={app}\lofiloop-app.exe
; FFmpeg TIDAK diperiksa keberadaannya di komputer pengguna: salinannya selalu disertakan.
; Installer jadi besar (56 MB), tetapi hasilnya bisa dipastikan dan tidak ada langkah
; tambahan bagi pengguna. Aplikasi memakai salinan ini lewat LOFILOOP_FFMPEG_DIR.

[Languages]
Name: "id"; MessagesFile: "compiler:Default.isl"

[Tasks]
; Menambahkan FFmpeg ke PATH berguna kalau pengguna ingin memakainya di terminal.
Name: "path"; Description: "Tambahkan FFmpeg ke PATH supaya bisa dipakai di terminal"; GroupDescription: "Pilihan tambahan:"

[Files]
Source: {#SourceExe}; DestDir: {app}; Flags: ignoreversion
#ifdef FfmpegDir
Source: {#FfmpegDir}\ffmpeg.exe; DestDir: {app}\ffmpeg; Flags: ignoreversion
Source: {#FfmpegDir}\ffprobe.exe; DestDir: {app}\ffmpeg; Flags: ignoreversion
#endif
Source: ..\README.md; DestDir: {app}; Flags: ignoreversion
Source: ..\LICENSE; DestDir: {app}; Flags: ignoreversion

[Icons]
Name: "{group}\LofiLoop"; Filename: "{app}\lofiloop-app.exe"
Name: "{autodesktop}\LofiLoop"; Filename: "{app}\lofiloop-app.exe"; Tasks: desktopicon
Name: "{group}\Copot LofiLoop"; Filename: "{uninstallexe}"

[Tasks]
Name: "desktopicon"; Description: "Buat pintasan di Desktop"

[Registry]
#ifdef FfmpegDir
; Aplikasi mencari FFmpeg di env ini lebih dulu, jadi tidak perlu mengubah PATH sistem.
Root: HKCU; Subkey: "Environment"; ValueType: string; ValueName: "LOFILOOP_FFMPEG_DIR"; \
    ValueData: "{app}\ffmpeg"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
    ValueData: "{olddata};{app}\ffmpeg"; Tasks: path; Check: NeedsAddPath('{app}\ffmpeg')
#endif

[Run]
Filename: "{app}\lofiloop-app.exe"; Description: "Buka LofiLoop sekarang"; \
    Flags: nowait postinstall skipifsilent

[Code]
// Menghindari PATH ganda kalau pengguna memasang dua kali.
function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

// Pemberitahuan di halaman pertama: FFmpeg selalu disertakan, tanpa memeriksa apakah
// komputer ini sudah punya. Itu disengaja supaya hasilnya bisa dipastikan dan pengguna
// tidak perlu memasang apa pun sendiri.
procedure InitializeWizard();
begin
#ifdef FfmpegDir
  WizardForm.WelcomeLabel2.Caption :=
    WizardForm.WelcomeLabel2.Caption + #13#10#13#10 +
    'Installer ini memasang LofiLoop beserta FFmpeg sekaligus, jadi tidak ada ' +
    'dependensi yang perlu kamu pasang sendiri.';
#endif
end;
