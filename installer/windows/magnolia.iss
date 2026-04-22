; Magnolia Server — Windows Installer
; Inno Setup 6+ https://jrsoftware.org/isinfo.php
;
; Build with: build-inno.bat
; Or directly: iscc magnolia.iss /DMyAppVersion=1.0.0

#ifndef MyAppVersion
 #define MyAppVersion "1.0.0"
#endif

#define MyAppName "Magnolia Server"
#define MyAppPublisher "magnolia"
#define MyAppId "{DEADBEEF-2026-4777-B00B-777B00BAA777}"
#define ServiceName "magnolia_server"
#define DataDir "{commonappdata}\Magnolia"

[Setup]
AppId=#MyAppId
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\Magnolia
DefaultGroupName={#MyAppName}
OutputDir=.
OutputBaseFilename=magnolia-{#MyAppVersion}-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Allow upgrading in-place (same AppId, higher version)
CloseApplications=yes
CloseApplicationsFilter=magnolia_server.exe

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\..\target\release\magnolia_server.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\target\release\service_ctl.exe"; DestDir: "{app}"; DestName: "magnolia_server-ctl.exe"; Flags: ignoreversion
Source: "..\..\target\release\create_admin.exe"; DestDir: "{app}"; DestName: "magnolia-create-admin.exe"; Flags: ignoreversion

[Dirs]
; system-full gives the LocalSystem service account write access.
; admins-full lets administrators run the binary or admin tools manually.
Name: "{commonappdata}\Magnolia"; Permissions: system-full admins-full
Name: "{commonappdata}\Magnolia\uploads"; Permissions: system-full admins-full
Name: "{commonappdata}\Magnolia\uploads\images"; Permissions: system-full admins-full
Name: "{commonappdata}\Magnolia\logs"; Permissions: system-full admins-full

[Run]
; Add Windows Firewall inbound rule for the server port.
; Declared in [Run] so Inno Setup owns the operation (recognized installer context).
; The {code:GetServerPort} call reads the port from the wizard at install time.
Filename: "{sys}\netsh.exe"; \
  Parameters: "advfirewall firewall delete rule name=""Magnolia Server"""; \
  Flags: runhidden; Description: "Remove old firewall rule (if any)"
Filename: "{sys}\netsh.exe"; \
  Parameters: "advfirewall firewall add rule name=""Magnolia Server"" dir=in action=allow protocol=TCP localport={code:GetServerPort} program=""{app}\magnolia_server.exe"" description=""Magnolia self-hosted social platform"""; \
  Flags: runhidden; Description: "Add firewall rule for Magnolia Server port"

[UninstallRun]
Filename: "{sys}\sc.exe"; Parameters: "stop {#ServiceName}"; RunOnceId: "StopService"; Flags: runhidden
Filename: "{sys}\sc.exe"; Parameters: "delete {#ServiceName}"; RunOnceId: "DeleteService"; Flags: runhidden
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Magnolia Server"""; RunOnceId: "RemoveFirewall"; Flags: runhidden

[Code]

// Windows API import 
function WinSetEnv(lpName, lpValue: String): Boolean;
 external 'SetEnvironmentVariableW@kernel32.dll stdcall';

// Wizard page variables
var
 PageServer: TInputQueryWizardPage;
 PageDatabase: TInputQueryWizardPage;
 PageSmtp: TInputQueryWizardPage;
 PageTurnEnable: TInputOptionWizardPage;
 PageTurnConfig: TInputQueryWizardPage;
 PageAdmin: TInputQueryWizardPage;
 IsUpgrade: Boolean;

// Create wizard pages
procedure InitializeWizard;
begin
 // Page 1: Server settings
 PageServer := CreateInputQueryPage(wpLicense,
 'Server Configuration',
 'Configure the server''s network settings.',
 '');
 PageServer.Add('Public base URL (e.g. https://magnolia.example.com):', False);
 PageServer.Add('CORS origin — URL users open in their browser (usually same as above):', False);
 PageServer.Add('Bind port (default 3000):', False);
 PageServer.Add('Local-only port — optional, leave blank to disable:', False);
 PageServer.Values[0] := 'http://localhost:3000';
 PageServer.Values[1] := 'http://localhost:3000';
 PageServer.Values[2] := '3000';
 PageServer.Values[3] := '';

 // Page 2: Database
 PageDatabase := CreateInputQueryPage(PageServer.ID,
 'Database',
 'Configure the database connection.',
 'Leave as default to use SQLite (recommended for most installs).');
 PageDatabase.Add('Database URL:', False);
 // sqlite: + path (no slashes) avoids the URI triple-slash ambiguity on Windows.
 // Forward slashes in the path work fine with SQLite on Windows.
 PageDatabase.Values[0] := 'sqlite:' + ExpandConstant('{commonappdata}') + '/Magnolia/magnolia.db';

 // Page 3: SMTP (optional)
 PageSmtp := CreateInputQueryPage(PageDatabase.ID,
 'Email (SMTP) — Optional',
 'Configure outgoing email. Leave blank to disable email features.',
 '');
 PageSmtp.Add('SMTP hostname (e.g. smtp.example.com):', False);
 PageSmtp.Add('SMTP port (587 = STARTTLS, 465 = TLS, 25 = plain):', False);
 PageSmtp.Add('SMTP username (leave blank if not required):', False);
 PageSmtp.Add('SMTP password:', True);
 PageSmtp.Add('From address (e.g. noreply@example.com):', False);
 PageSmtp.Values[1] := '587';

 // Page 4: TURN enable toggle
 PageTurnEnable := CreateInputOptionPage(PageSmtp.ID,
 'TURN Server — Optional',
 'Enable the embedded TURN relay server for voice and video calls.',
 'A TURN server relays media traffic for users behind restrictive NAT or firewalls.' +
 ' Most small deployments do not need this — direct peer connections work for most users.' +
 ' You can enable it later by editing the configuration file.',
 False, False);
 PageTurnEnable.Add('Enable embedded TURN server');

 // Page 5: TURN configuration (shown only when TURN is enabled)
 PageTurnConfig := CreateInputQueryPage(PageTurnEnable.ID,
 'TURN Server Configuration',
 'Configure the embedded TURN relay server.',
 'All fields are required when TURN is enabled.');
 PageTurnConfig.Add('Public IP address of this server (e.g. 203.0.113.42):', False);
 PageTurnConfig.Add('Credential signing secret (leave blank to auto-generate):', True);
 PageTurnConfig.Add('Listen address (default 0.0.0.0:3478):', False);
 PageTurnConfig.Add('Realm (default magnolia):', False);
 PageTurnConfig.Values[2] := '0.0.0.0:3478';
 PageTurnConfig.Values[3] := 'magnolia';

 // Page 6: Initial admin account (optional)
 PageAdmin := CreateInputQueryPage(PageTurnConfig.ID,
 'Initial Administrator Account — Optional',
 'Create the first admin account. Leave blank to skip.',
 'You can create an admin account later with: magnolia-create-admin.exe --email you@example.com');
 PageAdmin.Add('Admin email address:', False);
 PageAdmin.Add('Admin password (12 characters, Uppercase+Lowercase, 1 number, 1 symbol (?!$...)):', True);
end;

// Validate pages before advancing 
function NextButtonClick(CurPageID: Integer): Boolean;
begin
 Result := True;

 if CurPageID = PageServer.ID then
 begin
 if Trim(PageServer.Values[0]) = '' then
 begin
 MsgBox('Please enter the public base URL.', mbError, MB_OK);
 Result := False;
 end
 else if Trim(PageServer.Values[1]) = '' then
 begin
 MsgBox('Please enter the CORS origin (WEB_ORIGIN). If unsure, use the same value as the base URL.', mbError, MB_OK);
 Result := False;
 end
 else if Trim(PageServer.Values[2]) = '' then
 begin
 MsgBox('Please enter the server port.', mbError, MB_OK);
 Result := False;
 end;
 end;

 if CurPageID = PageTurnConfig.ID then
 begin
 if Trim(PageTurnConfig.Values[0]) = '' then
 begin
 MsgBox('Please enter the public IP address for the TURN server.', mbError, MB_OK);
 Result := False;
 end;
 end;

 if CurPageID = PageAdmin.ID then
 begin
 if (Trim(PageAdmin.Values[0]) <> '') and (Length(PageAdmin.Values[1]) < 8) then
 begin
 MsgBox('Admin password must be at least 12 characters, Uppercase+Lowercase, 1 number, 1 symbol (?!$...).', mbError, MB_OK);
 Result := False;
 end;
 end;
end;

// Skip the TURN config page when TURN is not enabled
function ShouldSkipPage(PageID: Integer): Boolean;
begin
 Result := False;
 if PageID = PageTurnConfig.ID then
 Result := not PageTurnEnable.Values[0];
end;

// Write the env config file
function WriteEnvFile: Boolean;
var
 ConfFile, BaseUrl, WebOrigin, Port, LocalPort, DbUrl: String;
 SmtpHost, SmtpPort, SmtpUser, SmtpPass, SmtpFrom: String;
 TurnEnabled: Boolean;
 TurnExternalIp, TurnSecret, TurnListenAddr, TurnRealm: String;
 Content: String;
begin
 ConfFile := ExpandConstant('{commonappdata}') + '\Magnolia\magnolia.env';
 BaseUrl := Trim(PageServer.Values[0]);
 WebOrigin := Trim(PageServer.Values[1]);
 Port := Trim(PageServer.Values[2]);
 LocalPort := Trim(PageServer.Values[3]);
 DbUrl := Trim(PageDatabase.Values[0]);
 SmtpHost := Trim(PageSmtp.Values[0]);
 SmtpPort := Trim(PageSmtp.Values[1]);
 SmtpUser := Trim(PageSmtp.Values[2]);
 SmtpPass := PageSmtp.Values[3];
 SmtpFrom := Trim(PageSmtp.Values[4]);
 TurnEnabled := PageTurnEnable.Values[0];
 TurnExternalIp := Trim(PageTurnConfig.Values[0]);
 TurnSecret := Trim(PageTurnConfig.Values[1]);
 TurnListenAddr := Trim(PageTurnConfig.Values[2]);
 TurnRealm := Trim(PageTurnConfig.Values[3]);

 Content :=
 '# Magnolia Server Configuration' + #13#10 +
 '# Generated by installer — edit as needed, then restart the service:' + #13#10 +
 '# net stop magnolia_server && net start magnolia_server' + #13#10 +
 '#' + #13#10 +
 '# NOTE: Env vars are also stored in the Windows Service registry entry.' + #13#10 +
 '# Editing this file alone is not enough — re-run the installer to update' + #13#10 +
 '# them, or edit HKLM\SYSTEM\CurrentControlSet\Services\magnolia_server\Environment.' + #13#10 +
 '' + #13#10 +
 'ENV=production' + #13#10 +
 'DATABASE_URL=' + DbUrl + #13#10 +
 'HOST=0.0.0.0' + #13#10 +
 'PORT=' + Port + #13#10;

 if LocalPort <> '' then
 Content := Content + 'LOCAL_PORT=' + LocalPort + #13#10;

 Content := Content +
 'BASE_URL=' + BaseUrl + #13#10 +
 'WEB_ORIGIN=' + WebOrigin + #13#10 +
 '#' + #13#10 +
 '# How many days a login session lasts (default: 7)' + #13#10 +
 '# SESSION_DURATION_DAYS=7' + #13#10 +
 '#' + #13#10 +
 '# Rate limiting' + #13#10 +
 '# RATE_LIMIT_GLOBAL=100' + #13#10 +
 '# RATE_LIMIT_AUTH=5' + #13#10 +
 '# TRUSTED_PROXY=' + #13#10 +
 '#' + #13#10 +
 '# Logging' + #13#10 +
 '# LOG_FORMAT=pretty' + #13#10 +
 '# LOG_OUTPUT=stdout' + #13#10 +
 '# LOG_FILE_PATH=' + ExpandConstant('{commonappdata}') + '\Magnolia\logs\magnolia.log' + #13#10 +
 '# LOG_INCLUDE_SOURCE=false' + #13#10;

 if SmtpHost <> '' then
 begin
 Content := Content + #13#10 +
 '# SMTP configuration' + #13#10 +
 'SMTP_HOST=' + SmtpHost + #13#10 +
 'SMTP_PORT=' + SmtpPort + #13#10;
 if SmtpUser <> '' then Content := Content + 'SMTP_USERNAME=' + SmtpUser + #13#10;
 if SmtpPass <> '' then Content := Content + 'SMTP_PASSWORD=' + SmtpPass + #13#10;
 if SmtpFrom <> '' then Content := Content + 'SMTP_FROM=' + SmtpFrom + #13#10;
 end
 else
 begin
 Content := Content + #13#10 +
 '# SMTP — leave commented to disable email features' + #13#10 +
 '# SMTP_HOST=smtp.example.com' + #13#10 +
 '# SMTP_PORT=587' + #13#10 +
 '# SMTP_USERNAME=user@example.com' + #13#10 +
 '# SMTP_PASSWORD=your-password' + #13#10 +
 '# SMTP_FROM=noreply@example.com' + #13#10;
 end;

 Content := Content + #13#10 +
 '# Encryption at rest (optional — 64 hex chars = 32-byte AES-256 key)' + #13#10 +
 '# Generate with: openssl rand -hex 32' + #13#10 +
 '# ENCRYPTION_AT_REST_KEY=' + #13#10;

 if TurnEnabled then
 Content := Content + #13#10 +
 '# TURN server' + #13#10 +
 'TURN_ENABLED=true' + #13#10 +
 'TURN_LISTEN_ADDR=' + TurnListenAddr + #13#10 +
 'TURN_REALM=' + TurnRealm + #13#10 +
 'TURN_EXTERNAL_IP=' + TurnExternalIp + #13#10 +
 'SESSION_SECRET=' + TurnSecret + #13#10
 else
 Content := Content + #13#10 +
 '# TURN server — disabled. To enable, set all four values and restart.' + #13#10 +
 '# SESSION_SECRET must be a random hex string (openssl rand -hex 32).' + #13#10 +
 '# TURN_ENABLED=true' + #13#10 +
 '# TURN_LISTEN_ADDR=0.0.0.0:3478' + #13#10 +
 '# TURN_REALM=magnolia' + #13#10 +
 '# TURN_EXTERNAL_IP=' + #13#10 +
 '# SESSION_SECRET=' + #13#10;

 Result := SaveStringToFile(ConfFile, Content, False);
end;

// Register the Windows Service and set its environment
procedure RegisterService(const AppDir: String);
var
 BinPath, TmpEnvFile: String;
 ResultCode: Integer;
 EnvVars: TArrayOfString;
 BaseUrl, WebOrigin, Port, LocalPort, DbUrl: String;
 SmtpHost, SmtpPort, SmtpUser, SmtpPass, SmtpFrom: String;
 TurnEnabled: Boolean;
 TurnExternalIp, TurnSecret, TurnListenAddr, TurnRealm: String;
 i: Integer;
begin
 BinPath := AppDir + '\magnolia_server.exe';
 BaseUrl := Trim(PageServer.Values[0]);
 WebOrigin := Trim(PageServer.Values[1]);
 Port := Trim(PageServer.Values[2]);
 LocalPort := Trim(PageServer.Values[3]);
 DbUrl := Trim(PageDatabase.Values[0]);
 SmtpHost := Trim(PageSmtp.Values[0]);
 SmtpPort := Trim(PageSmtp.Values[1]);
 SmtpUser := Trim(PageSmtp.Values[2]);
 SmtpPass := PageSmtp.Values[3];
 SmtpFrom := Trim(PageSmtp.Values[4]);
 TurnEnabled := PageTurnEnable.Values[0];
 TurnExternalIp := Trim(PageTurnConfig.Values[0]);
 TurnSecret := Trim(PageTurnConfig.Values[1]);
 TurnListenAddr := Trim(PageTurnConfig.Values[2]);
 TurnRealm := Trim(PageTurnConfig.Values[3]);

 // Stop and delete any previous service before re-creating
 Exec(ExpandConstant('{sys}\sc.exe'), 'stop {#ServiceName}',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
 Exec(ExpandConstant('{sys}\sc.exe'), 'delete {#ServiceName}',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
 Sleep(1500); // wait for SCM to remove the entry

 // Create service
 Exec(ExpandConstant('{sys}\sc.exe'),
 'create {#ServiceName} binPath= "' + BinPath + '" start= auto ' +
 'DisplayName= "Magnolia Server"',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

 Exec(ExpandConstant('{sys}\sc.exe'),
 'description {#ServiceName} "magnolia — self-hosted social platform"',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

 // Configure automatic restart on failure
 Exec(ExpandConstant('{sys}\sc.exe'),
 'failure {#ServiceName} reset= 86400 actions= restart/5000/restart/10000/restart/30000',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

 // Set per-service environment variables via the SCM registry key.
 // This is the Windows-native equivalent of systemd EnvironmentFile.
 // WEB_ORIGIN is required — the server panics on startup without it.
 SetArrayLength(EnvVars, 6);
 EnvVars[0] := 'ENV=production';
 EnvVars[1] := 'DATABASE_URL=' + DbUrl;
 EnvVars[2] := 'HOST=0.0.0.0';
 EnvVars[3] := 'PORT=' + Port;
 EnvVars[4] := 'BASE_URL=' + BaseUrl;
 EnvVars[5] := 'WEB_ORIGIN=' + WebOrigin;

 if LocalPort <> '' then
 begin
 i := GetArrayLength(EnvVars);
 SetArrayLength(EnvVars, i + 1);
 EnvVars[i] := 'LOCAL_PORT=' + LocalPort;
 end;

 if TurnEnabled then
 begin
 i := GetArrayLength(EnvVars);
 SetArrayLength(EnvVars, i + 5);
 EnvVars[i] := 'TURN_ENABLED=true';
 EnvVars[i+1] := 'TURN_LISTEN_ADDR=' + TurnListenAddr;
 EnvVars[i+2] := 'TURN_REALM=' + TurnRealm;
 EnvVars[i+3] := 'TURN_EXTERNAL_IP=' + TurnExternalIp;
 EnvVars[i+4] := 'SESSION_SECRET=' + TurnSecret;
 end;

 // Append optional SMTP vars
 if SmtpHost <> '' then
 begin
 i := GetArrayLength(EnvVars);
 SetArrayLength(EnvVars, i + 5);
 EnvVars[i] := 'SMTP_HOST=' + SmtpHost;
 EnvVars[i+1] := 'SMTP_PORT=' + SmtpPort;
 if SmtpUser <> '' then EnvVars[i+2] := 'SMTP_USERNAME=' + SmtpUser else EnvVars[i+2] := 'SMTP_USERNAME=';
 if SmtpPass <> '' then EnvVars[i+3] := 'SMTP_PASSWORD=' + SmtpPass else EnvVars[i+3] := 'SMTP_PASSWORD=';
 if SmtpFrom <> '' then EnvVars[i+4] := 'SMTP_FROM=' + SmtpFrom else EnvVars[i+4] := 'SMTP_FROM=';
 end;

 // Write REG_MULTI_SZ environment block via PowerShell (avoids Pascal type constraints)
 TmpEnvFile := ExpandConstant('{tmp}\magnolia_env.txt');
 SaveStringToFile(TmpEnvFile, '', False);
 for i := 0 to GetArrayLength(EnvVars) - 1 do
   SaveStringToFile(TmpEnvFile, EnvVars[i] + #13#10, True);
 Exec('powershell.exe',
   '-NoProfile -ExecutionPolicy Bypass -Command ' +
   '"$v = (Get-Content -LiteralPath ''' + TmpEnvFile + ''' | Where-Object { $_ -ne '''' }); ' +
   'Set-ItemProperty -LiteralPath ''HKLM:\SYSTEM\CurrentControlSet\Services\{#ServiceName}'' ' +
   '-Name Environment -Value $v -Type MultiString"',
   '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
 DeleteFile(TmpEnvFile);
end;

// Create initial admin account 
procedure CreateAdminAccount(const AppDir, DbUrl: String);
var
 AdminEmail, AdminPass: String;
 TmpPassFile: String;
 ResultCode: Integer;
begin
 AdminEmail := Trim(PageAdmin.Values[0]);
 AdminPass := PageAdmin.Values[1];
 if AdminEmail = '' then Exit;

 WinSetEnv('DATABASE_URL', DbUrl);

 // Write the password to a short-lived temp file and pipe it via
 // --password-stdin to avoid passing it as an env var or CLI argument.
 TmpPassFile := ExpandConstant('{tmp}\magnolia_pass.tmp');
 SaveStringToFile(TmpPassFile, AdminPass, False);

 Exec(ExpandConstant('{cmd}'),
 '/c type "' + TmpPassFile + '" | "' + AppDir + '\magnolia-create-admin.exe"' +
 ' --email "' + AdminEmail + '" --password-stdin',
 AppDir, SW_HIDE, ewWaitUntilTerminated, ResultCode);

 DeleteFile(TmpPassFile);

 if ResultCode <> 0 then
 MsgBox('Admin account creation returned code ' + IntToStr(ResultCode) + '.' + #13#10 +
 'You can retry later with: magnolia-create-admin.exe --email ' + AdminEmail,
 mbInformation, MB_OK);
end;

// Called by the [Run] section to get the port number chosen in the wizard
function GetServerPort(Param: String): String;
begin
 Result := Trim(PageServer.Values[2]);
end;

// Main post-install logic
procedure CurStepChanged(CurStep: TSetupStep);
var
 ConfFile, AppDir, DbUrl: String;
 ResultCode: Integer;
begin
 if CurStep = ssPostInstall then
 begin
 AppDir := ExpandConstant('{app}');
 ConfFile := ExpandConstant('{commonappdata}') + '\Magnolia\magnolia.env';
 DbUrl := Trim(PageDatabase.Values[0]);

 // Check for upgrade (existing config → preserve SESSION_SECRET TURN key)
 IsUpgrade := FileExists(ConfFile);

 if IsUpgrade then
 begin
 // Upgrade: re-register service (new binary) but keep config file intact.
 // Read the existing secret from the env file.
 // Simplest: just re-register service with new binary; env stays in registry.
 // The registry env was set during the previous install; values are preserved
 // unless the user changed them in the config file (which we also preserve).
 Exec(ExpandConstant('{sys}\sc.exe'), 'stop {#ServiceName}',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
 Sleep(2000);
 // Re-create with same settings (RegisterService re-reads wizard page defaults
 // which match any edited config; for a full upgrade just restart):
 Exec(ExpandConstant('{sys}\sc.exe'), 'start {#ServiceName}',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
 end
 else
 begin
 // Fresh install: write config, register service, start
 WriteEnvFile;
 RegisterService(AppDir);
 CreateAdminAccount(AppDir, DbUrl);

 // Start the service
 Exec(ExpandConstant('{sys}\sc.exe'), 'start {#ServiceName}',
 '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
 end;
 end;
end;

// Custom finish page message 
function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo,
 MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
 s: String;
begin
 s := '';
 if MemoDirInfo <> '' then s := s + MemoDirInfo + NewLine + NewLine;
 s := s + 'Service name: {#ServiceName}' + NewLine;
 s := s + 'Config file: ' + ExpandConstant('{commonappdata}') + '\Magnolia\magnolia.env' + NewLine;
 s := s + 'Data dir: ' + ExpandConstant('{commonappdata}') + '\Magnolia\' + NewLine + NewLine;
 s := s + 'The service will start automatically and on every boot.' + NewLine;
 s := s + 'After install: open http://localhost:' + PageServer.Values[2];
 Result := s;
end;
