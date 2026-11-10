# Raccolta Dati 2 — rules sheet

Original: `C:\Esprit\Raccolta Dati\Raccolta dati.exe` 10.11.1.0 (sha256 f1e652fa…), a .NET Framework
4.6.2 WinForms program. It is IL-only: the native engine and Ghidra see a single 6-byte function
(`_CorExeMain@thunk` at 0x6f0eea) and no UTF-16 user strings, so `native_strings_find` /
`native_function_pseudocode` cannot reach the code. The code was read as .NET instead
(managed_assembly_show ev_4b37fbe9…, managed_members_list ev_2bbe193c…, ILSpy C# + IL).
**Addresses below are method RVAs** ("Method begins at RVA") in the named assembly; constants were
checked in the IL bodies (`ldc.i4`, `ldstr`). Site values come from PRODUZIONE1, azienda MG (read-only).

### Native/Ghidra attempts (repeated on request)
- `core_target_open` → kind "managed" (.NET), target t_f1e652fa74b0.
- `native_functions_list` → 1 function only, `_CorExeMain@thunk` 0x6f0eea (ev_8e90906b…).
- `native_strings_find "BloccoCommesse"` / `"PD_PRIORLANCIO"` → 0 matches (ev_3239e602…,
  ev_586e88f4…): the .NET user strings (#US heap, UTF-16) are not in the native string index.
- `native_function_pseudocode` at `ESP_FormMain::RefreshCommesse` (RVA 0x1cce8, VA 0x41cce8) →
  "not in executable memory" / "could not disassemble": the method body is IL, not x86.
- `core_target_triage` (ev_31ea2693…): "This is a .NET assembly: use the .NET tools"; entry 0x6f0eea is
  the mscoree `_CorExeMain` stub, the only native code in the file.
- `managed_members_list` of `Raccolta_dati.ESP_FormMain` (ev_7c897db7…): 161 methods with IL bodies,
  8821 call sites — the source for the ESP_FormMain addresses and constants below.
So every value below was read from the IL with its method RVA (ILSpy `-il`, managed_members_list);
none was taken from memory of other programs.

## Architecture
- Reads: Rust, straight to SQL Server, same queries and rules as below, combined (one query per
  screen instead of one per card/phase).
- Saves: `bridge/RdPonte.exe` loads the original's own libraries (RD.Logic, RD.Dal.MSSql, Esprit.*)
  and replays each action in the original order; connection via `WinDevHelper.Inizializza`
  (RD.Logic 0x37ddc). Messages are the original's (`EspMessageBox.getTesto`). Yes/no boxes are
  asked in the new window and the action is re-run with the answer.

## Start-up — `Program::Main` (Raccolta dati 0x5d984), `ESP_FormLogin::B_Conferma_Click` (0x3fbd8)
- `C:\Esprit\Esprit.ini [Connect]` SERVERSQL / DATABASESQL / UIDSQL / PWDSQL (ldstr in Main).
  Empty server → Sigma Web API (APIUSER/APIPASSWORD/APIAZIENDA): not supported, shown on the splash.
- `[Raccolta dati]`: MaxLengthNoteCommessa default **80** (`ldc.i4.s 80`), BloccoCommesse
  (`int.Parse`, no default — the rebuild falls back to 4, *estimated*), MACCHINE and OPERATORI split
  on `,` (`ldc.i4.s 44`), DISPLAYRIDOTTO, TOUCH, InsParziale (`"S"`), REGISTRA_PESATE,
  USA_BADGE_OPERATORE, TORNA_SCHEDA_MACCHINE_DOPO_OPERAZIONI, CONSTRSQLINPUTDRIVER.
- `ESP_FormMain::.ctor` (0x1a054): `MACCHINE=*` → all CELPRO with CE_FLGFAMIGLIA='N',
  CE_TIPORISORSA='M'; the original then clicks the first machine (`Macchine_ButtonClick(Macchine[0])`
  0x1b3ec → `StatoRDDal::CreaSeInesistente` RD.Dal.MSSql 0xd63c), so its header shows whoever
  RD_MACCHINE has on that machine (often an operator working it from another PC).
  **Changed on request**: no machine is pre-selected and the app opens on OPERATORI; the header
  stays empty until the operator logs in and picks a machine (`--macchina <cod>` still pre-selects
  one for tests). Nothing is written at start-up.

## Reliable start (person's report: "opens only after many tries" on the work PC)
- Cause found: the program quit silently when the window/graphics (OpenGL) failed to open: no
  console, no log line (the log started after the window), no Windows crash event. The debug
  build was also ~440 MB (debug info) — Windows Defender scans a fresh file that size on its
  first launches — and opened a console window whose closing killed the app.
- Now: dev profile without debug info (`debug = false`, `strip = "debuginfo"`: ~42 MB); never a
  console (`windows_subsystem = "windows"`); avvio.log gets `processo N avviato (tentativo k)`,
  `apertura finestra…`, any window error and any crash (`ERRORE GRAVE: …` from a panic hook).
- If the window does not open (or the app stops with an error) it starts itself again, up to
  **6** consecutive attempts 400 ms apart (counter reset after 60 s of running); after the last
  one a Windows message gives the error and the log path. Test aid: `RD_PROVA_GUASTO=n` fails
  the attempts before n (seen: 2 simulated failures, then open at attempt 3).
- **Work-PC log (2026-11-10, copied to the project):** `processo 5476 avviato` → `apertura
  finestra…` → nothing more (no error, no crash line, never reached the app's own start-up
  steps); 4.5 s later a second launch found 5476 still holding the lock with **no window**;
  at 10:20 the same again. Cause: the window's graphics hang inside a driver. eframe 0.37's
  default renderer is wgpu, which probes DirectX 12, Vulkan **and** OpenGL
  (`Backends::PRIMARY | GL`), so any broken driver among them can block the start.
- Original for comparison (IL): `Program::Main` (0x5d984) only calls
  `Application.EnableVisualStyles()` and `SetCompatibleTextRenderingDefault(false)` — a WinForms
  program drawing with GDI, never through a GPU driver, so it cannot hang there. Hence the last
  mode below needs no graphics driver either. Native check: `native_strings_find
  "EnableVisualStyles|SetCompatibleTextRenderingDefault|opengl|d3d|directx"` → 0
  (ev_17461b7b679b…); Ghidra entry 0x6f0eea = `_CorExeMain` jump (ev_c6fec7dac3c3…).
- Fix — graphics modes, one backend at a time: **dx12** (wgpu, `WGPU_BACKEND=dx12`) →
  **opengl** (eframe glow renderer) → **vulkan** (wgpu) → **software** (wgpu DX12 with the
  software adapter WARP, `DeviceType::Cpu`, chosen by `native_adapter_selector`; no GPU
  driver involved; opens in ~0.25 s here). A watchdog thread waits up to **12 s**
  for the first drawn frame (`finestra_pronta`, called from `App::ui`); if it does not come it
  logs `nessuna finestra dopo 12 s con grafica X (driver bloccato)`, starts a copy in the next
  mode and ends the stuck process at once (`TerminateProcess`, a hung driver may never
  return). The mode that drew a frame is kept in `%LOCALAPPDATA%\RaccoltaDati\grafica.txt`
  (`RD_GRAFICA` overrides) and used first next time. Tested here: simulated hang in dx12
  (`RD_PROVA_BLOCCO=dx12`) → restart in opengl after 12 s → window open, opengl remembered;
  each mode opens in < 0.4 s on this PC. A second launch while a copy has no window yet logs
  that the copy restarts itself.
- **Second work-PC log (10:39–10:40, after the watchdog version):** many attempts logged
  "apertura finestra…" and then **vanished 5–11 s later with no log line** (before the 12 s
  watchdog), until one opened in 0.4 s in the same dx12 mode. A process killed from outside or
  crashing in a driver cannot report itself, and a second click had also shown the hidden
  window of a copy still starting.
- Fix — **launcher**: the program the operator starts holds the one-copy lock, has no window
  and never touches graphics; it starts the app as a child (`RD_FIGLIO=1`, `RD_GRAFICA`,
  `RD_TENTATIVO`) and waits for `%LOCALAPPDATA%\RaccoltaDati\finestra.ok` (the child's pid,
  written at its first drawn frame). Child ends first → log `chiusa senza finestra dopo x s:
  codice 0x…` with the meaning (0xC0000005 memory/driver, 0xC0000409 fail-fast, 1 stopped from
  outside, 2 window error, 3 program error) → next graphics mode; no frame in 12 s → the
  launcher kills it ("bloccata") → next mode; up to **8** attempts, then a Windows message.
  After the window is up the launcher waits for the app to end (logging a non-zero code) and
  exits. A second launch only brings a **visible** window to the front. RIAVVIA starts a fresh
  launcher (RD_FIGLIO/RD_GRAFICA/RD_TENTATIVO removed). Tested: simulated silent crash in dx12
  (`RD_PROVA_CRASH=dx12`) → code 0xC0000409 logged → opengl opened in 0.4 s; simulated hang
  (`RD_PROVA_BLOCCO=dx12`) → killed at 12 s → opengl opened; release build opens dx12 in 0.4 s.
- Original's runtime and log (IL / config): `Raccolta dati.exe.config` → `supportedRuntime v4.0,
  .NETFramework v4.6.2`; the launcher logs the installed .NET Framework 4.x "Release" against
  **394802** (= 4.6.2; read with RegGetValueW from HKLM\SOFTWARE\Microsoft\NET Framework
  Setup\NDP\v4\Full), since the save module runs on those libraries. The original's own error
  log: `ESP_FormLogin::.ctor` (0x3fa08) → `new FileLogger(Application.UserAppDataPath +
  "/logRaccoltaDati.txt")`, with AssemblyCompany "Esprit", AssemblyProduct "Raccolta dati",
  version 10.11.1.0 → `%APPDATA%\Esprit\Raccolta dati\10.11.1.0\logRaccoltaDati.txt` (named in
  LEGGIMI.txt). Native check: `native_strings_find "WriteException|Finalizze|.log|
  ExceptionHandler"` → 0 (ev_1a2136c12deb…); Ghidra entry 0x6f0eea = `_CorExeMain` jump.
- Version of the original: native `native_strings_find "10.11.1|FileVersion|ProductVersion"` →
  5 matches in `.rsrc` (ev_4c11fa078a91…): FileVersion "10.11.1.0" at 0x6f25a8, ProductVersion
  "10.11.1.0" at 0x6f2700 (also AssemblyVersion 10.11.1.0 in IL). Every rule here was read from
  that version, and saves run on the original's libraries of the PC in use, so the launcher
  reads `RD_LIBDIR\Raccolta dati.exe`'s VS_FIXEDFILEINFO (GetFileVersionInfoW) and logs
  "stessa versione" or **ATTENZIONE … verificare i salvataggi** when it differs, or "non
  trovato" when missing.
- One copy at a time: `%LOCALAPPDATA%\RaccoltaDati\istanza.lock` kept open without sharing; a
  second launch brings the open window to the front and exits (RIAVVIA and the automatic
  restart wait up to 10 s for the old copy to close). `--screenshot` runs are exempt.
- The save module (RdPonte.exe) ends on its own when the app's pipe closes, so a crashed copy
  leaves nothing running.
- Original for comparison (IL): `Program::Main` (0x5d984) has no single-copy check (no Mutex or
  process lookup) and no `ThreadException`/`UnhandledException` handler — a start-up error shows
  .NET's own crash dialog; `IniFile::.ctor` (0x5c68b) throws FileNotFoundException without the
  ini; after the company login (`ESP_FormLogin.ShowDialog() == OK`) it runs `ESP_FormMain`, whose
  `.ctor` (0x1a054, line 691) sets `WindowState = FormWindowState.Maximized` with the normal
  border (no full screen). The rebuild keeps "maximised" as the default; single copy, crash
  log, automatic retry and full screen are additions. Native check: `native_strings_find
  "Mutex|FormBorderStyle|WindowState|Maximized|GetProcessesByName|ThreadException|
  UnhandledException"` → 0 (ev_cf6727502e56…); Ghidra entry 0x6f0eea = `_CorExeMain` jump
  (ev_c6fec7dac3c3…).

## After recording quantities (network load)
- `bNewPezzi_Click` (0x220bc) → `RegistraPezzi` (0x22164) → `MostraMascheraVersamento` (0x2228c):
  shows `ESP_FormPezziRag`, then, with `refreshDgPezzi: true`, calls only `RefreshPezzi`
  (0x22420) — no `RefreshAmbiente`, phases or orders reload. The rebuild now does the same
  (one query, `carica_versati`) instead of reloading the whole screen (7 queries). The automatic
  barcode flow still reloads once at its end, after restoring the machine.
- Native check: `native_strings_find "bNewPezzi|ESP_FormPezziRag|RefreshPezzi|RefreshAmbiente|
  GetNewConn"` → 0 (ev_b2a76c59612b…); Ghidra entry 0x6f0eea = `_CorExeMain` jump
  (ev_c6fec7dac3c3…).

## Package for the work PC (`pacchetto\RaccoltaDati-Windows` + `.zip`)
- `RaccoltaDati.exe` = `cargo build --release` (≈37 MB, no console), `RdPonte.exe` from
  `bridge/out`, `LEGGIMI.txt` (install, graphics modes, where the log is). Not included:
  Esprit.ini (credentials, stays on the work PC) and the original DLLs (`C:\Esprit\Raccolta
  Dati\`, already there). Release build checked: window in 0.3 s (dx12), save module started.

## Full screen (person's request)
- First card of IMPOSTAZIONI: SÌ/NO applies at once; SALVA writes `SCHERMO_INTERO=Y|N` in
  `[Raccolta dati]` (a key only this app reads; the original reads just the keys listed below)
  and the next start opens full screen; CHIUDI without saving goes back to the file's value.
  F11 switches it for the session. Without it the window opens maximised as before.

## New: settings editor for Esprit.ini (login screen, person's request)
- IMPOSTAZIONI (bottom right of the login card screen, no password) opens a full-screen editor of
  the same file the original reads (`--ini`/`RACCOLTA_DATI_INI`, default `C:\Esprit\Esprit.ini`;
  `Program::Main` 0x5d984 reads it through `iniFile.IniReadValue`).
- Every line, comment, blank line and key order is kept; an unchanged line is written back byte
  for byte; a changed one becomes `KEY=value`. Same encoding (UTF-8 or ANSI) and line endings.
  Before writing, a copy `Esprit.ini.bak-YYYYMMDD-HHMMSS` is made next to it; the new file is
  written to `.tmp` and swapped in. Read-only runs can look but not save.
- The comment lines above a key (the file's own notes) are its description. Yes/no keys the
  original compares with "Y" (TOUCH, DISPLAYRIDOTTO, USA_BADGE_OPERATORE, USERNAME_PSW_STRING,
  TORNA_SCHEDA_MACCHINE_DOPO_OPERAZIONI, REGISTRA_PESATE, MesRidotto, AutoRefreshCommesse,
  MOSTRA_INPUT_NON_ASSEGNATI, or any value Y/N) get SÌ/NO buttons; InsParziale (compared with
  "S") gets S/N. Other values open the touch keyboard with an extra row `. : _ \ / = ; -` and
  SPAZIO. Values with PWD/PASSWORD in the key or `Password=` inside are hidden until MOSTRA
  PASSWORD. Changes apply from the next start: RIAVVIA starts a new copy and closes this one.
- Original's ini code (IL): `IniFile::.ctor` (0x5c68b) fixes the path `C:\espRIT\espRIT.ini` and
  throws if missing; `IniReadValue` (0x5c6d8) = `GetPrivateProfileString(section, key, "", buf, 255,
  path)` → a value reaches the original cut to **254** characters, so the editor caps values at 254
  (`impostazioni::MAX_VALORE`) and says so; `IniWriteValue` (0x5c6c7, WritePrivateProfileString)
  exists but is never called — the original only reads the file.
- Keys and comparisons of `Program::Main` (0x5d984), used for the editor's switches and notes:
  [Connect] DATABASESQL, SERVERSQL, UIDSQL, PWDSQL; [Raccolta dati] MaxLengthNoteCommessa
  (TryParse, else 80), DISPLAYRIDOTTO == "Y", BloccoCommesse (int.Parse), MACCHINE / OPERATORI
  (Split ','), CONSTRSQLINPUTDRIVER, REGISTRA_PESATE == "Y", MOSTRA_INPUT_NON_ASSEGNATI **!= "Y"**,
  TOUCH == "Y", AutoRefreshCommesse == "Y", CONSTROASI, SCHEMAOASI, InsParziale == **"S"**,
  FermiMacchinaDay == **"S"**, ArticoloLabel, MesRidotto == "Y", PRINTER_EXE_PATH,
  USERNAME_PSW_STRING == "Y", USA_BADGE_OPERATORE == "Y", TORNA_SCHEDA_MACCHINE_DOPO_OPERAZIONI ==
  "Y", IDWSINS. Keys of this list missing from the file are shown as "Non presente nel file" rows
  in their section and written (after the section's last key) only once given a value; keys
  without a note in the file get a short description from this list.
- Native check: `native_strings_find "IniReadValue|IniWriteValue|GetPrivateProfileString|
  WritePrivateProfileString|Esprit.ini"` → 0 (ev_4722ecf6e6ee…); Ghidra entry 0x6f0eea =
  `_CorExeMain` jump (ev_c6fec7dac3c3…). Rules from IL.
- Test option: `--impostazioni` opens the editor at start.

## Touch scrolling of the lists (person's request)
- Machines and orders lists scroll by dragging with the finger, in addition to ▲/▼. egui 0.37's
  default drag scrolling (`DragScroll::OnTouch`) stays off when Windows reports the touch screen
  as a mouse, so every list uses `ScrollSource::ALL` (drag always on); a tap still selects.
- ▲/▼ on MACCHINE move one row of cards (the original's `bMacchineSu_Click`/`bMacchineGiu_Click`
  move `panel2.VerticalScroll` by LargeChange). On COMMESSE the whole list is one scroll; ▲/▼ move
  by one block of `BloccoCommesse` cards and "Pag. x / y" shows the block at the top.
- The other lists (operators, phases, tables, quantity screen, pick lists) also drag-scroll.

## Look: flat (person's request, test)
- `ui::PIATTO = true`: buttons, tabs and cards are one solid colour (the darker shade while
  pressed), no drop shadows, no gloss on the traffic lights; `false` restores gradients/shadows.
- Original for comparison: `ESP_FormMain::InitializeComponent` (0x28bb0) never sets `FlatStyle`
  and gives 71 controls `UseVisualStyleBackColor = true` (plain Windows system buttons); no
  assembly of the original uses `LinearGradientBrush`, so it paints no gradients or gloss of its
  own — the flat look is closer to it. Native check: `native_strings_find
  "FlatStyle|Gradient|LinearGradientBrush|UseVisualStyleBackColor"` → 0 (ev_fad8cd6c6ed5…);
  Ghidra entry 0x6f0eea = `_CorExeMain` jump (ev_c6fec7dac3c3…).

## New: locked until operator login (person's request)
- The original has no login gate: `Program::Main` (0x5d984) opens `ESP_FormMain` straight after the
  company choice, with every tab and the first machine's data (`.ctor` 0x1a054).
- Here, until an operator logs in on this PC the screen shows only the company logo, the exit
  button, the clock and an "Accesso operatore" card with ACCEDI: no header data, tabs, BARCODE,
  actions, database or company code. ACCEDI is the original's INIZIA TURNO login
  (`RaccoltaDatiHelper::SelezioneOperatore` 0x4a374 checks, then `IniziaTurno` 0x1b848).
- An operator whose shift is already open is let in and the shift goes on (the original shows
  "Operatore già collegato" and selects the operator, `Inizia_Turno_Click` 0x1b788) — needed so
  an operator can get back in after a restart. Logged-in operators join the exit logout list.
- After login the MACCHINE screen opens. When the logged-in operator ends the shift (FINE TURNO)
  the app locks again.
- **PRENDI MACCHINA** moved from OPERATORI to the MACCHINE action bar, next to "Operatore: … /
  Macchina: …"; it runs the original's `bControllaMacchina_Click` (0x1c5dc) for the logged-in
  operator on the selected machine (enabled once a machine is selected).
- Caption: `InitializeComponent` sets `bControllaMacchina.Text = "Controlla Macchina"`; the `.ctor`
  (0x1a054, IL line 622) replaces it with `Settings.LABEL_CONTROLLO_MACCHINA` when not blank
  (`Settings::.ctor` RD.Logic 0x317a8: `LeggiSetupString("LABEL_CONTROLLO_MACCHINA", "")`). The
  build reads the same setting and shows it in capitals; MG has it empty
  (`spCN_SETUPGetSingleValue`, row 1827), so the button reads PRENDI MACCHINA.
- Before writing, the action keeps the original's checks (`TurnoAperto` 0x1c9ec, called first by `bControllaMacchina_Click` 0x1c5dc):
  no operator → message 162; operator's shift no longer open (e.g. ended on another PC) → 163;
  no machine → 160.
- Native check of this round: `native_strings_find
  "LABEL_CONTROLLO_MACCHINA|bControllaMacchina|Operatore già collegato|TurnoAperto"` → 0
  (ev_8bf0afa35511…); Ghidra entry 0x6f0eea = `_CorExeMain` jump (ev_c6fec7dac3c3…). Rules from IL.
- Test option (read-only runs only, so it can never skip the login when saving is possible):
  `--operatore <cod>` starts as if that operator had logged in.

## New: session logout on exit (person's request)
- The original writes nothing when closing: `ESP_FormMain::Esci_Click` (0x1a6c1) closes the SQL
  connection and exits; `FormMain_FormClosing` (0x24e36) only flushes the log.
- This app remembers the operators who **started a shift** (INIZIA TURNO) or **took a machine**
  (PRENDI MACCHINA) on this PC. ESCI and the window's close button ask for confirmation, naming them,
  then end each one's shift with the original's own end-of-shift action `turno_fine` =
  `FineTurnoOperatoreSelezionato` (0x1b8f4): question 468 to suspend running phases when
  SOSPENDI_FASI_FINE_TURNO=Y (MG: Y), TM record, `TurnoDal::FineTurno`, and
  `StatoRDDal::TogliOperatoreDaMacchine` (RD.Dal.MSSql 0xd8e0) — which is what releases the machine
  control. Only then does the window close.
- If ending a shift fails (e.g. "No" to 468 → message 417 listing the running phases), a window
  explains it and offers ANNULLA (stay) or ESCI COMUNQUE (leave the operator logged in).
- An operator who ends the shift with FINE TURNO leaves the list. Operators only logged in on
  another PC are never touched. Read-only mode never logs anyone out.

## Operator code and password with letters
- `RaccoltaDatiHelper::SelezioneOperatore` (0x4a374) uses `FormInputStringa` (letters, RD.Logic
  0x33bc) for code and password when `[Raccolta dati] USERNAME_PSW_STRING=Y`
  (`Program::Main` line 80), else the number pad `FormInputNumerico`. This workstation's ini has
  no USERNAME_PSW_STRING, but some operator codes contain letters (e.g. MODELLERIA, TAGLIO), so on
  the person's request the code and password always use a letters-and-digits touch keyboard.
  Its keys are the original's `FormInputStringa` (RD.Logic): `InitializeComponent` (0x34c4) lays out
  1–0, Q…P, A…L, Z…M as **capitals only**, one separator key whose caption the `.ctor` (0x33bc) sets
  to `CultureInfo.CurrentCulture.NumberFormat.NumberDecimalSeparator` (**","** on Italian Windows),
  delete (`bDelete_Click` 0x3434: remove last char), OK (`bOk_Click` 0x3480: Value = text) and
  Esci (`bClose_Click` 0x262c: cancel); `bTasto_Click` (0x3411) appends the key caption; no
  MaxLength; `pwd` → `PasswordChar = '*'` (the rebuild masks with `*` too). Added on touch only: an
  "abc" key for lower case and a C (clear) key; a physical keyboard or badge reader also types
  into it. Native check of this round: `native_strings_find
  "USERNAME_PSW_STRING|Codice Operatore|Devi inserire un codice|UTENTE NON TROVATO|Password
  Operatore"` → 0 (ev_eb9fc4ae38c6…); Ghidra pseudocode of entry 0x6f0eea is the `_CorExeMain`
  jump (ev_c6fec7dac3c3…), so these rules come from the IL. The checks after it are unchanged (empty → "Devi inserire un codice", OPERATORI
  filter → "UTENTE NON TROVATO", unknown user, password).
- Test option: `--accesso` opens the operator login at start.
- Timers (`ESP_FormMain::InitializeComponent` 0x28bb0): clock **1000 ms**, machine data
  **2000 ms** (timerAggiornamentoDati), SPC and phase-close checks 3000 ms (not rebuilt).
- New: splash with timed steps, log `%LOCALAPPDATA%\RaccoltaDati\avvio.log`.
- Network: `RD.Logic SQLServer::GetConnectionString` (0x1e6a8) builds
  `Server=…;Database=…;User Id=…;Password=…;TrustServerCertificate=True;` with no timeout or pool
  options, and `getNewConn` (0x1e74f) opens a new connection per operation — so the original runs
  on Microsoft.Data.SqlClient defaults (Connect Timeout **15 s**, pooling on). The rebuild uses
  the same 15 s to reach/connect to the server (`db::CONNECT_TIMEOUT`, was an estimated 8 s) and
  keeps up to 6 open connections for its reads. Measured on this LAN (avvio.log 2026-11-06):
  server reached in ~22 ms, login ~52 ms, settings ~80 ms, each list well under 100 ms; saves go
  through the original's libraries (bridge, started in ~0.5 s once). Native check:
  `native_strings_find "Connect Timeout|Connection Timeout|Pooling|CommandTimeout|Data Source"` → 0
  (ev_5aaaa70ec290…); Ghidra entry 0x6f0eea = `_CorExeMain` jump (ev_c6fec7dac3c3…).

## Settings — `RD.Logic.Settings::.ctor` (RD.Logic 0x317a8), `impostazioni::SetImpostazioni` (0x2d5a8)
Defaults in IL / MG values: NDECQTA 0 / **3**; ORD_COMME_MES "P" / P; DAYS_SHOW_COMMESSE **999**;
MAX_COMMESSE_CARICATE_MES 0 / 0; BLOCCA_SE_SEMAFORO_ROSSO "Y" / **N**; AVANZORDINEFASI "OF";
CAUSALE_VERS_FASE_ULTIMA "VF"; CAUSALE_VERS_FASE_INTERM "VS"; OPERAZIONE_LAVORO_SU_FASE "ORE";
OPERAZIONE_LAVORO_TURNI "TM"; CAUSALE_CONTROLLO_MACCHINA "CMAC"; PRELIEVO_MATERIALI Y;
VERSA_SU_SINGOLE Y (MG); SOSPENDI_FASI_FINE_TURNO Y (MG); MOSTRA_CONFERMA_FASE Y;
VISUALIZZA_CONFERMA_CHIUSURA_FASE_PEZZI Y; FASI_FAMIGLIA_UNA_MACCHINA Y (MG).
Read through `spCN_SETUPGetSingleValue` (SetupDal.GetSingleValue).

## Header — `ESP_FormMain::RefreshAmbiente` (0x1a8bc)
RD_MACCHINE ⨝ CELPRO ⨝ Users ⨝ PD_COMM_T ⨝ PD_FASI for the machine: commessa "R anno - numero",
operator NOME, machine DESCR, phase PD_DESFAS, article "cod - descr" cut to 74 chars when > 75.

## Machines — `ESP_FormMain::RefreshMacchine` (0x1ae4c)
CELPRO (DESCR, IDREPCDC, CE_FAMIGLIA trimmed, CE_AVANZAFASI, CE_VERSA_SOLO_BUONI…) + RD_MACCHINE.

## Operators — `ESP_FormMain::RefreshOperatori` (0x1c218)
Open shifts: RD_TURNI ⨝ Users where RD_ORAFIN is null; time shown "HH:mm"; filtered by OPERATORI.

## Orders — `ESP_FormMain::RefreshCommesse` (0x1cce8), `CommessaRD::.ctor` (0x3b38)
- R orders not closed, PD_DATCOM ≤ today + DAYS_SHOW_COMMESSE or PD_PRIORLANCIO < 999, with an open
  phase on the machine or its family; kept only if `FaseDal::CheckFasiNonChiuseCommeMac`
  (RD.Dal.MSSql 0xbde4).
- Quantities: `Funzioni::GetNumPezziDaProdurre` (RD.Logic 0x21844) = SUM(PD_QUANTI);
  `Funzioni::GetNumPezzi` (0x2134c) = SUM(PD_NPEZZI) of VF (only finished goods) on the machine and
  in total; scrap `Scarto::GetScartiComme` (0x30ab4) = operations with PD_SCARTO='I'.
- Article: `CommessaRD::set_CodArticolo` (0x39f4): `GestoreCommesse::GetDescrArticoloCommessa`
  (0x13958, free-text items → PD_NOTE), empty → `Funzioni::getDescrClass1` (0x23a84, MG_CLASS1).
- Format: `Funz::NullToEmptyAndTruncate` (Esprit.Funzioni 0x328c) pattern `"0."` padded with `'0'`
  (`ldc.i4.s 48`) to the decimals → "68,000".
- Traffic light `Funzioni::getColoreCommessa` (0x22164): green if on this machine
  (`isCommessaInCorsoSuMacchina` 0x22474); preceding orders (`getCommessePrecedenti` 0x24144) all
  started (`isCommessaIniziata` 0x210e0) and, when PD_PERCENTUALE > 0, produced/qty×**100** ≥
  percentage, and at **100** also closed (`isCommessaTerminata` 0x2254c) → then
  `PrimaFaseSuMacchinaProntaAPartire` (0x22368) not red → yellow; first preceding started → light
  blue; else red.
- Phase colour `Funzioni::getColoreFase` (0x219f8): started → green, finished → white; no previous
  phase (`getProgrFasePrec` 0x21f44 = −1) → yellow; PD_PERCSOV = **100** →
  `GestoreFasi::IsTempoFaseRegistrato` (0x150d8, ORE rows); else previous pieces / qty × **100**
  ≥ 100 − PERCSOV or previous finished (`isFaseFinita` 0x21e28) → yellow; else red.
- Sort `GetOrderingColoreCommessa` (0x1d7c8): green **1**, yellow **2**, light blue **3**, white **4**,
  red **5**, other **99**; then PD_ORDLANCIO (null → 9999999999), priority/date ("P") or
  date/priority ("D"), year, number. BLOCCA_SE_SEMAFORO_ROSSO hides non green/yellow.
  Cap MAX_COMMESSE_CARICATE_MES. Pages of CommessePerBlocco (`AggiungiCommePannello` 0x1d69c,
  `bAvanti_Click` 0x1e448, `bIndietro_Click` 0x1e5b4).
- Filter `chFiltroComme_Click` (0x1cbe8): "Anno-Numero"; a single order maps to its group.

## Phases — `ESP_FormMain::RefreshFasi` (0x1e750)
- Phases of the order on this machine, PD_TYP ≠ 5 (NASCONDI_FASI_TERMINATE hides finished).
- Theoretical time: production phases (`Funzioni::IsFaseDiProduzione` 0x23181: PD_TYP "2" or "4")
  **3600 × qty ÷ PD_QTAORA** s, others **3600 ÷ PD_QTAORA** s.
- Missing pieces: InsParziale → max(0, qty − pieces of the phase), else qty − pieces.
- Recorded time `Funzioni::GetDurataRegistrataFase` (0x28084) = SUM(PD_TOTSEC) of ORE (+ running
  time); forecast = time / pieces on machine × missing (production) or theoretical − recorded.
- State: finished / running (started, not suspended) / to do.

## Actions (bridge, original order and messages)
| Action | Original | Address |
|---|---|---|
| Start shift | `Inizia_Turno_Click` → `RaccoltaDatiHelper::SelezioneOperatore` → `IniziaTurno` → `TurnoDal::InizioTurno` | 0x1b788, 0x4a374, 0x1b848, RD.Dal 0xe158 |
| End shift | `FineTurnoOperatoreSelezionato` (ask 468, suspend phases, TM, `TurnoDal::FineTurno`) | 0x1b8f4, RD.Dal 0xe208 |
| Take machine | `bControllaMacchina_Click` (CMAC, `StatoRDDal::ImpostaOperatoreSuMacchina`) | 0x1c5dc, RD.Dal 0xd6c0 |
| Start order | `CheckInizioCommessa` (169/170/171) + `IniziaCommessa_Click` | 0x1e318, 0x1d920 |
| Suspend order | `bSospendiCommessa_Click` (167) | 0x1e0b0 |
| End (orders tab) | `bTerminaCommessa_Click_1` = `ChiudiSospendiFase('T')` | 0x1e2f0, 0x1fc18 |
| Start phase | `IniziaFase_Click` (175/176/177/178, `GestoreFasi::IniziaFase`) | 0x1f328, RD.Logic 0x149a8 |
| Suspend / end phase | `ChiudiSospendiFase` → `terminaFase_ControllaValori` (182/183/184) → `fase_ControlloChiusuraFasePezziVersati` (453) → close window → `terminaFase_TerminaFase` (466, 188/189, 164, `Commessa::ChiudiCommessa`) | 0x1fc18, 0x26948, 0x28308, 0x4a704, 0x26bf8, RD.Logic 0x1a570 |
| Family phases | `getListaFasiAperteMacchineDiFamiglia` | 0x27580 |
| Use phase | `bUtilizzaFase_Click` (191) | 0x1fdc4 |
| Record pieces | `bNewPezzi_Click` (206/207) → `ESP_FormPezziRag::.ctor` / `CaricaDGVersamenti` / `b_Conferma_Click` / `GetArticoliDaVersare` → `ESP_FormConfConsumiMat::b_Conferma_Click` → `VersamentoProcessor` | 0x220bc, 0x4ab10, 0x4b2cc, 0x4ca6c, 0x4e3f0, 0x465d8, RD.Logic 0x36f04 / 0x378b4 |
| Output operation | `RaccoltaDatiHelper::GetCodOperazioneVersamenti`: order "OF" → VF on the last production phase, else VS | 0x4a624 |

- Close-phase window (`ESP_FormChiudiSospFase::InitializeComponent` 0x18eac): operators 1 decimal,
  step **0.5** (decimal 5, scale 1), max **9**; hours max **9 999 999 999**; minutes max **59**.
  Defaults (`MostraPopUpChiusiSospendiFase` 0x4a704): hours = floor(total hours), minutes =
  minutes part, end time cut to the minute.
- Quantity rows (`CaricaDGVersamenti` 0x4b2cc): one row per single order with VERSA_SU_SINGOLE;
  "Versa tutto" (`bVersaTutto_Click` 0x4f3cc) = quantity to do, or what is missing when
  VERSA_PEZZI_RIMANENTI; entry rounded to the item decimals (`dgPezzi_CellValueChanged` 0x4ef70);
  custom unit PD_LABEL/PD_COEFFICIENTE.
- Recorded list `RefreshPezzi` (0x22420): VF on the last production phase (`FaseFunz::IsLastLavFase`
  0x1fb38: no later phase of type 2/4), else VS. Items `CaricaArticoliVers_Prel` (0x23d84).
  Machine data `CaricaGridDatiMacchine` (0x24ed0, PD_DATIMACCHINA, refreshed by
  timerAggiornamentoDati 2000 ms) is **not shown**: the person removed the DATI tab; its slot in the
  tab row is the BARCODE button. IL check that nothing else is lost: `CaricaGridDatiMacchine` only
  reads (`SELECT DESCRIZIONE,VALORE,DATAMOD FROM PD_DATIMACCHINA WHERE ID_AZIENDA=@AZIENDA AND
  PD_IDMACCHINA=@IDMACCHINA`, no machine → message 160); `timerAggiornamentoDati_Tick` (0x25104)
  reloads it only `if tabControl1.SelectedTab == pDatiMacchina` (IL_0014); `tabControl1_Selected`
  (0x1a6d4, case "pDatiMacchina", IL_01d4) only starts that timer and loads the grid. Native check
  (round of the BARCODE change): target t_f1e652fa74b0 has one native function, `_CorExeMain@thunk`
  0x6f0eea (ev_4601645835f0…, pseudocode ev_e53c29cc480a…: a tail call into the .NET runtime);
  `native_strings_find "PD_DATIMACCHINA"` → 0 (ev_9956f7150222…) and
  `barcode|scanner|lettore|timerAggiornamentoDati|CaricaGridDatiMacchine` → 0 (ev_f45424adfcee…) —
  the strings live in the IL #US heap, so the rules above come from the IL.

## New: BARCODE button and listening window (not in the original)
- The reader is **off** by default. The BARCODE button (last slot of the tab row; also in the
  quantity screen's tool row) opens a window over a dimmed backdrop that listens. Only while that
  window is on top and not searching are characters from the reader (keyboard wedge) collected;
  a code is taken on Enter/Tab, or after a 300 ms pause when it is 5+ characters (*estimated* pause
  for readers without a suffix). INSERISCI A MANO types the code on the keypad (same as a scan);
  ANNULLA closes it (and drops a search in progress — nothing is written before the search ends).
- Success closes the window and stops listening: on the main screens when the search finds at
  least one order (then the automatic flow below continues); on the quantity screen when a row
  matches (then the confirmation card / list below). No match or a search error stays in the
  window with the reason shown and listens again.
- The button is disabled while the automatic flow or a save is running.
- Colour (person's request): orange **#F57C00** with a darker **#C45600** gradient and white text.
  The original's only orange is the SPC alarm: `ESP_FormMain::timerSPC_Tick` (RVA 0x25120) sets
  `statusStrip1.BackColor = Color.OrangeRed` (255,69,0) when a machine has FlgFareMisurazioneSPC,
  else `Color.White`. SPC is not rebuilt, and the button's orange is kept clearly apart from that
  red-orange so it never reads as an alarm. Native check: Ghidra (Deep engine) pseudocode of the
  only native function, entry 0x6f0eea, is a jump into `_CorExeMain` (ev_c6fec7dac3c3…);
  `native_strings_find "orange|arancio|DarkOrange|BackColor"` → 0 (ev_0f7e2aa7a5bd…): the colour is
  in IL, not native data.
- Like the original's machine screens (`CaricaGridDatiMacchine` 0x24ed0: `AppState.CodMacchina == null`
  → message 160), a scan with no machine selected does nothing but ask to select the machine.
- No reader code exists in the original (0 strings for barcode/scanner/lettore, ev_f45424adfcee…),
  so the 300 ms pause stays an estimate.

## New: PIÙ COMMESSE — batch of the automatic flow (person's request) — `src/app/lotto.rs`
- In the BARCODE window (main screens) the orange PIÙ COMMESSE button switches to list mode.
  Each scan is only **verified** (reads): `cerca_singola` on the machine being controlled
  (same rule as the single flow); several matches → pick one; then machine state, the group's
  phases on the machine (`dati::fasi`) and the target phase (`fase_bersaglio`: running
  production phase, else first not finished); pieces already recorded (`versato_singola`, VF+VS,
  minus scrap when CE_VERSA_SOLO_BUONI) → **max = quantity − recorded** with InsParziale=S
  (as `Versamento.GetArticoliDaProdurre` RD.Logic 0x35270), else the quantity. max = 0 →
  refused ("già versata completamente"); same order twice → refused. Quantity preset to max;
  −/+ or tap to type, kept within 1…max; ✕ removes a row.
- AVVIA groups the orders by group order R (in scan order) and runs the automatic flow **once per
  group**: open the order on the machine if needed (`commessa_inizia`), start/use the phase,
  open the quantity screen (`vers_carica`), fill **every** order of the group on its row
  (never above what the screen itself proposes: `proposta`, VERSA_PEZZI_RIMANENTI) and write
  them in **one** `vers_conferma` (ESP_FormPezziRag::b_Conferma_Click 0x4ca6c saves all rows),
  then put the machine back as in the single flow. A group that cannot run (no operator, the
  machine on another order, an error or a "No" to a question) is recorded and the next group
  goes on. At the end one summary: "N commesse registrate su M" plus each failure and why.
- Mixing group orders: `ESP_FormMain::CheckInizioCommessa` (0x1e318) refuses to start an order
  while another is on the machine (`AppState.AnnoComm/NumComm` set and different → message
  169; 165 none selected; red light 170/171). The batch puts the machine back as it found it
  after each group, so: machine **free** → orders of any group orders can be mixed; machine
  **on an order** → only that order's singles. The scan now refuses the others at once (and
  refuses when the machine has no operator, `bNewPezzi_Click` 0x220bc → 174) instead of
  failing at AVVIA. Native check: `native_strings_find "CheckInizioCommessa|
  IniziaCommessa_Click|bSospendiCommessa|RD_ANNOCOM"` → 0 (ev_c84b334bd79d…).
- One save for several rows is the original's own behaviour: `ESP_FormPezziRag::b_Conferma_Click`
  (0x4ca6c) collects every filled row with `GetArticoliDaVersare` (0x4e3f0) into one
  `VersamentoProcessor` (`AddArticoloDaVersare(commessa S, codArt, qty)` per row with
  VERSA_SU_SINGOLE) and `GeneraVersamentiProposti` writes them together. Per-row checks there:
  qty > "da fare" → BLOCCA_VERSAMENTO_MAGGIORE_COMMESSA "Y" blocks (460), "F" asks (459) and
  needs the VERSQTAMAG permission, "N" allows (MG: **N**, spCN_SETUPGetSingleValue row 1753);
  previous-phase check → 471; serial-tracked items need whole quantities → 150; no row → 461.
  The batch never exceeds what the screen proposes, so 460/459 cannot occur; a 471 fails that
  group and is listed in the summary.
- Native check: `native_strings_find "b_Conferma|ESP_FormPezziRag|bVersaTutto|
  GetArticoliDaProdurre|VERSA_PEZZI_RIMANENTI"` → 0 (ev_35f97317d871…); Ghidra entry 0x6f0eea =
  `_CorExeMain` jump (ev_c6fec7dac3c3…). Rules from IL.
- Read-only: collecting works, AVVIA only says what would happen. Test option:
  `--lotto c1,c2,…` (with `--operatore`, `--macchina`).
- Not yet tried on a real save (the save path is the single flow's, with several rows).

## New: INFO COMMESSA (person's request) — reads only
- Purpose (person): know **which machine of this workstation** an order number is on.
- Workstation machines = the same list the app shows (`dati::macchine`): Esprit.ini MACCHINE
  codes, or with `MACCHINE=*` every CELPRO row with CE_CANCELLATO='N', CE_FLGFAMIGLIA='N',
  CE_TIPORISORSA='M' (`ESP_FormMain::.ctor` 0x1a054); family = CE_FAMIGLIA trimmed
  (`RefreshMacchine` 0x1ae4c, `ImpostaAppStateMacchina`). Native check: `native_strings_find
  "CE_FAMIGLIA|PD_CODFAMIGLIA|CodMacchinePostazione|CE_FLGFAMIGLIA"` → 0 (ev_ca351d55b5ea…);
  Ghidra entry 0x6f0eea = `_CorExeMain` jump (ev_c6fec7dac3c3…). Rules from IL.
- Icon-only search button (teal `theme::INFO` #00838F, 104 px) next to BARCODE (after login).
  Opens the same listening window in "info" mode; a scan (or INSERISCI A MANO) runs
  `dati::info_codice`: single orders (PD_TIPOCOM 'S', open or closed) whose number matches like
  the barcode flow (exact → ends with → contains; year part narrows) **and whose group order has
  a phase on a machine of Esprit.ini MACCHINE or on one of their families** (PD_FASI.PD_CODMAC
  IN codes ∪ families, as `RefreshCommesse` 0x1cce8 IL_00c2 matches `@PD_CODMAC OR
  @PD_CODFAMIGLIA`); open ones first, details for the best 5. Only those machines' phases are
  listed, the machine (description; a family code shows the workstation machines in it) first:
  order state (PD_DATAC), its group R (PD_ANNOCOMRAG/PD_NUMCOMRAG, closed or not), article and
  quantity (PD_COMM_D), the machines whose RD_MACCHINE has the group now, and every phase of the
  group except type 5 (PD_FASI ⨝ PD_TIPIFASI: progressive, description, machine, state from
  PD_DTEINI/PD_DTEFIN) with the pieces recorded for the scanned order on production phases
  (types 2/4; VF+VS from PD_DATIPROD, as `Versamento.GetArticoliDaProdurre` counts them).
- Phase state, same rule as the phase list (`dati::stato_fase`, from `ESP_FormMain::RefreshFasi`
  0x1e750, IL reading PD_DTEINI/PD_DTESOSP/PD_DTEFIN): PD_DTEFIN set → Terminata; PD_DTEINI set
  and PD_DTESOSP null → InCorso; everything else → **DaFare** — a started-then-suspended phase is
  DaFare for the original. Here it is labelled "Sospesa" (FASI) / "sospesa · da fare" (INFO) as
  extra detail, but every rule (`fase_bersaglio`, automatic flow) treats it as DaFare.
  Native check: `native_strings_find "PD_DTESOSP|PD_DTEINI|PD_DTEFIN|PD_DATAC|
  ESP_FormParametriCommessa"` → 0 (ev_a792902909457…); Ghidra entry 0x6f0eea = `_CorExeMain`
  jump (ev_c6fec7dac3c3…).
- No match → reason in the window, listening again; found → result window with ALTRO CODICE
  (listen again) and CHIUDI. Only SELECT queries; the save module is never called.
- Test option: `--info <code>` (with `--operatore`, read-only).

## Barcode matching on the quantity screen
- Match on the single orders of the screen: "105266", "20105266", "26-20105266", "2026-20105266";
  exact number first, then numbers ending with the code, then containing it; a year part must
  match the end of the year; fewer than 3 digits are ignored.
- One match → confirmation card with the quantity pre-filled with the order's quantity to do
  (what is still missing when VERSA_PEZZI_RIMANENTI, as `bVersaTutto_Click` 0x4f3cc); −/+ or tap
  to type; CONFERMA fills the row (rounded to the item decimals) and highlights it. Several
  matches → list to pick from. None → reason in the barcode window. The record is still written
  only by CONFERMA.
- Test options: `--versa` opens the quantity screen, `--scansiona <code>` simulates one scan in the
  barcode window, `--barcode` opens the barcode window.

## New: automatic versamento from one scan (BARCODE window on the main screens) — `src/app/rapido.rs`
Built only on the original's actions, in the original order, through the save helper:
1. **Find**: open single orders (PD_TIPOCOM 'S', PD_DATAC null) of an open group (R) whose number
   matches the scan (exact → ends with → contains; year part narrows), and whose group has an open
   phase on a machine of this workstation or its family. Several → list; none → message.
2. **Machine**: only the machine being controlled (selected on screen), with the same rule as the
   original's order list `ESP_FormMain::RefreshCommesse` (RVA 0x1cce8): an unfinished phase with
   `PD_CODMAC = @PD_CODMAC OR PD_CODMAC = @PD_CODFAMIGLIA` (IL_00c2) that also passes
   `FaseDal::CheckFasiNonChiuseCommeMac` (IL_0337, RD.Dal.MSSql 0xbde4: its progressive is not
   already finished on this machine). Other workstation machines are ignored. Native check:
   `native_strings_find "PD_CODFAMIGLIA"` → 0 (ev_3c8013a6…), IL only.
   Stops (nothing written) when the machine has no operator (174) or works another order (169).
   **Already complete** (checked before anything is started): with InsParziale=S the original's
   `Versamento.GetArticoliDaProdurre` (RD.Logic 0x35270) lists only what is still to record on the
   phase (IL: `ldarg.s insParziale` IL_016d → `Funzioni::GetNumPezzi(…, SoloVersamentiMag=false)`
   IL_01c3, minus `Scarto::GetQtaScartataArticoloFase` IL_01e9 only when `macchinaSoloPezziBuoni`,
   negative → 0 IL_01f9), so when that done quantity on the phase the flow would use ≥ the single's
   quantity, the flow stops with a message and writes nothing. Native check: `native_strings_find
   "InsParziale"` → 0 (ev_1dbaf425…), IL only (seen on 2026-40108222: 20 of 20 on ORLATURA3
   phase 40, whole group 696/696).
3. **Open the order** if the machine has none: `commessa_inizia` = CheckInizioCommessa +
   IniziaCommessa_Click (0x1e318, 0x1d920) with the order's traffic light (asks 171 if needed).
4. **Phase**: the running production phase (made "in use" with bUtilizzaFase_Click 0x1fdc4 if
   needed), else the first unfinished production phase (types 2/4) started with IniziaFase_Click
   (0x1f328).
5. **Quantity**: the quantity screen (bNewPezzi_Click 0x220bc) opens with the scanned order matched
   and its quantity pre-filled; one CONFERMA writes it (ESP_FormPezziRag::b_Conferma_Click 0x4ca6c).
6. **Back**: undo only what the flow started — suspend the phase it started (ChiudiSospendiFase 'R'
   0x1fc18; the close window's proposal of hours/minutes/operators is accepted as is), put back the
   phase that was in use, release the order it opened (bSospendiCommessa_Click 0x1e0b0, allowed
   because `GestoreFasi::SospendiFase` RD.Logic 0x14d70 clears PD_DTEINI) — then the starting
   machine and screen. Cancelling at any point, an error or a "No" to an original question also
   undoes the steps already done.
- IL check of the "back" rule: `ESP_FormMain::bSospendiCommessa_Click` (RVA 0x1e0b0) counts
  `PD_FASI … PD_DTEINI IS NOT NULL AND PD_DTEFIN IS NULL` for the machine (IL_0080) → message 167,
  else `RDMacchineFunz::SettaCommessaSuMacchina` (IL_0156) with empty order; `GestoreFasi::SospendiFase`
  (RD.Logic RVA 0x14d70) calls `FaseDal::AggiornaDTInizio(faseKey, null)` (IL_0015) and
  `TerminaDPFase` (IL_0041). Native check of the same method (VA 0x41e0b0) and of the SQL text: no
  function / 0 strings (ev_080a787f…) — IL only.
- Read-only mode: the search runs and says what would happen; nothing is written.
- Test option: `--scansiona <code>` (without `--versa`) opens the barcode window on the main screen
  and simulates a scan in it.

## Deviations
- An operator already on shift is not opened a second time (the original warns "Operatore già
  collegato" and inserts another open shift).
- Not in this version (message, use the original): consumption/withdrawal confirmation, lot or
  serial items, moulding, weighing, SPC sessions, anomalies/stops, scrap entry, size breakdown
  (never forced by the original: `Versamento.PezziTaglie` is pre-filled). MG data: 0 scrap,
  0 consumption, 0 VF rows with lots or sizes.
