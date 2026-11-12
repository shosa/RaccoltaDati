# Raccolta Dati — touch MES client for Esprit

A rebuild of Esprit's **Raccolta dati** (version 10.11.1.0) for touch monitors on the shop floor:
same database, same rules, same records, with a bright touch UI and a few new automations
(barcode flow, multi-order batch, INFO COMMESSA). Written in Rust (eframe/egui 0.37).

- **Operators:** see [GUIDA-OPERATORE.md](GUIDA-OPERATORE.md) (Italian).
- **Every rule and where it comes from in the original:** [MECHANICS.md](MECHANICS.md).
- **Values with sources:** [mechanics.json](mechanics.json).

---

## 1. How it is put together

```
RaccoltaDati.exe  ── launcher (no window) ──► RaccoltaDati.exe (the app, child process)
                                                 │  reads      ──► SQL Server (tiberius, pooled)
                                                 │  writes     ──► RdPonte.exe (C#, x86)
                                                 │                   └─► original Esprit DLLs
                                                 │                       C:\Esprit\Raccolta Dati\
                                                 └─ settings   ──► C:\Esprit\Esprit.ini
```

**Reads** go straight to SQL Server with queries ported from the original. **Every write**
(shifts, machines, orders, phases, quantities) goes through `RdPonte.exe`, a small C# program
that calls the original's own libraries (`RD.Logic.dll`, `RD.Dal.MSSql.dll`, …). This is on
purpose: the database rows are produced by Esprit's code, not by a re-implementation.

### Source map

| File | What it does |
|---|---|
| `src/main.rs` | Launcher (one-copy lock, graphics modes, restarts), test options, window |
| `src/avvio.rs` | Start-up steps, splash screen, `avvio.log` |
| `src/config.rs` | `Esprit.ini` reader, `[Raccolta dati]` workstation settings |
| `src/db.rs` | SQL Server pool, query builder (`Sql`, `@P` params), `Loader`/`Task` |
| `src/dati.rs` | All queries: machines, shifts, orders, phases, quantities, day's records, barcode search, INFO |
| `src/ponte.rs` | Talks to `RdPonte.exe` over stdin/stdout |
| `src/theme.rs`, `src/ui.rs` | Colours, fonts, Phosphor icons, touch tiles, cards |
| `src/app.rs` | App state, login lock, tabs, header, saving, exit/logout |
| `src/app/schermi.rs` | The screens (machines, operators, orders, phases, pieces, articles) |
| `src/app/dialoghi.rs` | Dialogs, keyboards, quantity screen, barcode window, INFO result |
| `src/app/rapido.rs` | Automatic barcode flow (one order) |
| `src/app/lotto.rs` | PIÙ COMMESSE batch (many orders, one run per R order) |
| `src/app/impostazioni.rs` | Esprit.ini editor (login screen) |
| `bridge/*.cs` | `RdPonte.exe`: the original's save actions |
| `build.rs` | Compiles the bridge with Windows' own C# compiler |

---

## 2. Building

Requirements on the build PC:

- Rust stable, target `x86_64-pc-windows-gnu` (the `.cargo/config.toml` points `dlltool` to
  WinLibs; adjust the path if WinLibs is elsewhere).
- The original program installed in `C:\Esprit\Raccolta Dati\` (or set `RD_LIBDIR`), because
  `build.rs` compiles `bridge/*.cs` against its DLLs with
  `C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe`, as **x86** (the original's DLLs are
  32-bit). Without them the app still builds, but saving is unavailable.

```
cargo build              # development build (target/debug)
cargo build --release    # what goes to the work PCs (~37 MB, no console)
cargo test               # unit tests (barcode matching, quantities, ini editor, phase state…)
```

The bridge ends up in `bridge/out/RdPonte.exe`.

### Package for a work PC

Copy `target/release/raccolta_dati.exe` as `RaccoltaDati.exe` and `bridge/out/RdPonte.exe`
**into the same folder**, add `LEGGIMI.txt`. The project's `pacchetto/` folder holds the
current one. Never ship `Esprit.ini` (credentials) or the original DLLs (already on the PC).

---

## 3. Running and testing

The app reads `C:\Esprit\Esprit.ini` (or `--ini <path>`, or `RACCOLTA_DATI_INI`).

### Test options (safe: combine with `--sola-lettura`)

| Option | Effect |
|---|---|
| `--sola-lettura` | Nothing is ever written (the bridge only answers reads) |
| `--screenshot <file.png>` | Draws ~120 frames, waits for data, saves a picture, exits (always read-only) |
| `--operatore <cod>` | Starts as if that operator had logged in (read-only runs only) |
| `--macchina <cod>` | Pre-selects a machine |
| `--scheda <nome>` | Opens a tab: `operatori`, `macchine`, `commesse`, `fasi`, `produzione`, `articoli`, `versamenti` |
| `--accesso` | Opens the operator login keyboard |
| `--barcode` | Opens the barcode window |
| `--scansiona <code>` | Simulates a scan (with `--versa`: in the quantity screen) |
| `--versa` | Opens the quantity screen of the machine |
| `--info <code>` | INFO COMMESSA lookup |
| `--lotto c1,c2,…` | Builds a PIÙ COMMESSE list from these codes |
| `--impostazioni` | Opens the Esprit.ini editor |

Example: `raccolta_dati.exe --operatore TAGLIO --macchina TAGLIO2 --lotto 20110911,20110912 --screenshot t.png`

### Environment variables

| Variable | Use |
|---|---|
| `RD_LIBDIR` | Folder of the original DLLs (default `C:\Esprit\Raccolta Dati`) |
| `RD_PONTE` | Path of `RdPonte.exe` (default: next to the app, else `bridge/out`) |
| `RD_GRAFICA` | Force a graphics mode: `dx12`, `opengl`, `vulkan`, `software` |
| `RD_PROVA_CRASH=<mode>` / `RD_PROVA_BLOCCO=<mode>` | Simulate a silent crash / a hung driver in that mode |
| `RD_ICONE=1` | Shows every icon large (drawing check) |
| `RD_FIGLIO`, `RD_TENTATIVO`, `RD_RIAVVIO` | Internal: launcher ↔ app |

---

## 4. Start-up, logs and troubleshooting

The program the operator clicks is a **launcher**: it holds the one-copy lock
(`%LOCALAPPDATA%\RaccoltaDati\istanza.lock`), checks .NET Framework (≥ 4.6.2, release 394802)
and the version of the installed original, then starts the app as a child and waits for its
first drawn frame (`finestra.ok`). If the app dies or shows no window in **12 s**, the launcher
logs Windows' exit code and restarts it in the next graphics mode
(**dx12 → opengl → vulkan → software**), up to **8** attempts. The working mode is kept in
`grafica.txt`. In Task Manager two `RaccoltaDati.exe` processes are normal.

Files in `%LOCALAPPDATA%\RaccoltaDati\`:

| File | Content |
|---|---|
| `avvio.log` | Every start: launcher, .NET, original version, attempts, exit codes, each start-up step with timing (server, login, settings, machines, save module) |
| `grafica.txt` | Graphics mode that worked last (delete or edit to force another) |
| `istanza.lock`, `finestra.ok` | Launcher bookkeeping |

The original's own error log: `%APPDATA%\Esprit\Raccolta dati\10.11.1.0\logRaccoltaDati.txt`.

| Symptom | Look at |
|---|---|
| Nothing appears | `avvio.log`: `tentativo n di 8 … codice 0x…` lines; `0xC0000005` = driver/memory, `1` = stopped from outside (antivirus) |
| Splash stops at a step | That step's line (server unreachable, login, settings…) |
| "Modulo di salvataggio non trovato" | `RdPonte.exe` must be next to `RaccoltaDati.exe`; DLLs in `C:\Esprit\Raccolta Dati\` |
| `ATTENZIONE … verificare i salvataggi` | The PC has a different version of the original than 10.11.1.0 |

---

## 5. What we learned about the original

The original is a .NET Framework 4.6.2 WinForms program; all its logic is in IL (the native part
is only the `_CorExeMain` stub), so it was studied by decompiling the IL. The details, with
method addresses, are in [MECHANICS.md](MECHANICS.md). The essentials:

**Settings.** `Esprit.ini` is read with `GetPrivateProfileString` (values cut at **254**
characters) and never written by the original. `[Raccolta dati]`: `BloccoCommesse` (orders per
page, mandatory), `MACCHINE` (codes or `*`), `OPERATORI` (empty = all), flags compared with
`"Y"` (TOUCH, DISPLAYRIDOTTO, USA_BADGE_OPERATORE, USERNAME_PSW_STRING…) except **InsParziale**
and FermiMacchinaDay (`"S"`) and MOSTRA_INPUT_NON_ASSEGNATI (`!= "Y"`). Company settings come from
`CN_SETUP` through `spCN_SETUPGetSingleValue` (MG: NDECQTA 3, VERSA_SU_SINGOLE Y,
BLOCCA_SE_SEMAFORO_ROSSO N, BLOCCA_VERSAMENTO_MAGGIORE_COMMESSA N, AVANZORDINEFASI OF,
operations VF/VS/ORE/TM/CMAC).

**Data model.** Group orders are type **R**, single orders type **S** (`PD_ANNOCOMRAG /
PD_NUMCOMRAG` link an S to its R). Phases are `PD_FASI` (types 2 and 4 = production); a phase is
*Terminata* with `PD_DTEFIN`, *In corso* with `PD_DTEINI` and no `PD_DTESOSP`, otherwise *Da fare*.
Recorded pieces are `PD_DATIPROD` rows (VF on the last production phase, VS on the others).
Machine state is `RD_MACCHINE` (operator, order, phase in use); shifts are `RD_TURNI`.

**Rules that matter in daily use.**
- An order list shows R orders with an open phase on the machine **or its family**
  (`CE_FAMIGLIA`), not already finished on that machine.
- A machine can work only one order: starting another is refused (message 169).
- With InsParziale=S the quantity screen lists only what is still missing (quantity − recorded
  VF+VS, minus scrap for "versa solo buoni" machines).
- Several orders are saved by **one** confirmation (`ESP_FormPezziRag` writes all filled rows
  together) — which is what PIÙ COMMESSE relies on.
- Suspending a phase clears its start date, which is what lets the automatic flow release the
  order afterwards and leave the machine as it found it.
- Ending a shift (`FineTurnoOperatoreSelezionato`) suspends running phases (question 468), writes
  the TM record and removes the operator from every machine.
- The original has no single-copy check, no crash log and opens maximised; it draws with plain
  Windows graphics (GDI), so it never depends on the video driver.

**Differences on purpose** (listed in MECHANICS.md): login gate before any data; operator login
with letters; closing the app logs out the operators of this PC; no machine pre-selected; DATI
tab removed; barcode window, automatic flow, PIÙ COMMESSE, INFO COMMESSA; settings editor;
launcher with graphics fallback.

---

## 6. Open points

- Saves go through the original's libraries, but a side-by-side check that the rows are
  identical to the original's has not been done on a live order yet.
- Sizes (taglie), lot-tracked items and material consumptions are not entered here (unused at
  MG in the last 90 days); the app points to the original for them.
- PIÙ COMMESSE has only been tested up to the save (test runs are read-only).
