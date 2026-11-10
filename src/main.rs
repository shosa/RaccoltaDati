// Never a console window next to the app (closing it would close the app too).
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod avvio;
mod config;
mod dati;
mod db;
mod ponte;
mod theme;
mod ui;

use std::path::PathBuf;

use eframe::egui;

const TITOLO: &str = "Raccolta Dati";

/// Window icon: the same 3×3 tile mark as the header.
fn icon() -> egui::IconData {
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    let cell = n / 3;
    for y in 0..n {
        for x in 0..n {
            let (i, j) = (y / cell, x / cell);
            if i > 2 || j > 2 || x % cell >= cell - 3 || y % cell >= cell - 3 {
                continue;
            }
            let c = theme::lerp(egui::Color32::from_rgb(56, 132, 230), theme::PRIMARY_DARK, (i + j) as f32 / 4.0);
            rgba[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&[c.r(), c.g(), c.b(), 255]);
        }
    }
    egui::IconData { rgba, width: n as u32, height: n as u32 }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
}

/// One copy at a time. A second click while the first copy is still starting (or already
/// open) brings that window to the front instead of starting another copy (two copies
/// would each start their own save module). The lock is a file kept open without sharing.
fn istanza_unica() -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let p = avvio::log_path().with_file_name("istanza.lock");
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let apri = || std::fs::OpenOptions::new().create(true).truncate(false).write(true).share_mode(0).open(&p);
    let mut esito = apri();
    // RIAVVIA (settings): the copy that started us is closing, wait for it (max ~10 s)
    if std::env::var("RD_RIAVVIO").is_ok() {
        for _ in 0..50 {
            if !matches!(&esito, Err(e) if e.raw_os_error() == Some(32)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            esito = apri();
        }
    }
    match esito {
        Ok(f) => Some(f),
        // ERROR_SHARING_VIOLATION: another copy holds it
        Err(e) if e.raw_os_error() == Some(32) => {
            let titolo: Vec<u16> = TITOLO.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                // a window still being created is hidden: never show it early, just leave
                let h = FindWindowW(std::ptr::null(), titolo.as_ptr());
                let h = if h != 0 && IsWindowVisible(h) != 0 { h } else { 0 };
                avvio::log(&if h != 0 {
                    format!("---- già aperto: porto in primo piano la finestra esistente (processo {}) ----", std::process::id())
                } else {
                    format!("---- già in avvio: l'altra copia sta aprendo la finestra e riprova da sola (processo {}) ----", std::process::id())
                });
                if h != 0 {
                    // SW_RESTORE only when minimised, so a maximised window stays maximised
                    ShowWindow(h, if IsIconic(h) != 0 { 9 } else { 5 });
                    SetForegroundWindow(h);
                }
            }
            std::process::exit(0);
        }
        // any other problem with the lock must not stop the app
        Err(_) => None,
    }
}

/// Automatic starts of the app when its window does not appear (two rounds of the modes).
const MAX_TENTATIVI: u32 = 8;

#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

// ---------------------------------------------------------------------------
// Start-up. Seen on a work PC (avvio.log 2026-11-10, twice): "apertura finestra…" and then
// nothing; the process either stayed alive without a window, or vanished 5–11 s later with
// no error and no crash line, until one attempt opened in 0.4 s in the same mode. A crash
// in a driver or a security tool stopping the process cannot be caught from inside it.
// So the program the operator starts is only a LAUNCHER (no window, holds the one-copy
// lock): it starts the app as a child process, waits for its first drawn frame, and when
// the child dies or does not draw in time it logs Windows' exit code and starts it again,
// with the next graphics mode. One click is enough.

/// In order of preference: wgpu on DirectX 12 only, OpenGL (glow), wgpu on Vulkan only, and
/// last "software": DirectX 12's software adapter (WARP, DeviceType::Cpu), which needs no
/// graphics driver — like the original, a WinForms program drawing with GDI
/// (Program::Main 0x5d984: Application.EnableVisualStyles + SetCompatibleTextRenderingDefault).
const MODI_GRAFICI: [&str; 4] = ["dx12", "opengl", "vulkan", "software"];
/// Seconds the window has to draw its first frame.
const ATTESA_FINESTRA: u64 = 12;

static FINESTRA_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MODO_ATTUALE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn file_grafica() -> PathBuf {
    avvio::log_path().with_file_name("grafica.txt")
}

/// RD_GRAFICA (set by an automatic restart) → the mode that worked last time → dx12.
fn modo_grafico() -> String {
    let m = std::env::var("RD_GRAFICA").ok().or_else(|| std::fs::read_to_string(file_grafica()).ok()).map(|m| m.trim().to_lowercase()).unwrap_or_default();
    if MODI_GRAFICI.contains(&m.as_str()) { m } else { MODI_GRAFICI[0].to_string() }
}

fn modo_successivo(m: &str) -> &'static str {
    let i = MODI_GRAFICI.iter().position(|x| *x == m).unwrap_or(0);
    MODI_GRAFICI[(i + 1) % MODI_GRAFICI.len()]
}

/// Written by the app at its first drawn frame (its process id): the launcher waits for it.
fn file_finestra() -> PathBuf {
    avvio::log_path().with_file_name("finestra.ok")
}

/// First frame drawn: the mode works, keep it for the next starts and tell the launcher.
pub fn finestra_pronta() {
    if !FINESTRA_OK.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let m = MODO_ATTUALE.get().cloned().unwrap_or_default();
        avvio::log(&format!("finestra aperta (grafica {m})"));
        if !m.is_empty() && std::env::var("RD_SCREENSHOT").is_err() {
            let _ = std::fs::write(file_grafica(), &m);
            let _ = std::fs::write(file_finestra(), std::process::id().to_string());
        }
    }
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(hkey: isize, subkey: *const u16, value: *const u16, flags: u32, kind: *mut u32, data: *mut u32, size: *mut u32) -> i32;
}

/// The save module runs on the original's libraries, which need .NET Framework 4.6.2:
/// `Raccolta dati.exe.config` → `<supportedRuntime version="v4.0" sku=".NETFramework,Version=v4.6.2"/>`.
/// 4.6.2 = "Release" 394802 under HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full.
const NET_462: u32 = 394_802;

fn net_framework() -> Option<u32> {
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (chiave, valore) = (w(r"SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full"), w("Release"));
    let (mut dato, mut dim) = (0u32, 4u32);
    // HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD
    let r = unsafe { RegGetValueW(0x8000_0002u32 as i32 as isize, chiave.as_ptr(), valore.as_ptr(), 0x10, std::ptr::null_mut(), &mut dato, &mut dim) };
    (r == 0).then_some(dato)
}

/// What a Windows exit code says about how the app ended.
fn spiega_codice(c: u32) -> &'static str {
    match c {
        0 => "chiusa normalmente",
        1 => "fermata dall'esterno (antivirus, criteri di sicurezza o Gestione attività)",
        2 => "la finestra non si è aperta (errore grafico)",
        3 => "errore del programma (vedi ERRORE GRAVE)",
        0xC000_0005 => "accesso non valido alla memoria (spesso il driver grafico)",
        0xC000_0409 => "arresto immediato del processo (fail-fast)",
        0xC000_00FD => "stack overflow",
        0xC000_0142 => "una libreria di sistema non si è inizializzata",
        0xC000_001D => "istruzione non supportata dal processore",
        _ => "chiusura inattesa",
    }
}

/// The program started by the operator: one-copy lock, then the app as a child process,
/// watched and restarted (next graphics mode) until its window draws.
fn lanciatore() {
    let _istanza = istanza_unica();
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            avviso_windows(&format!("Raccolta Dati: percorso del programma non leggibile: {e}"));
            return;
        }
    };
    avvio::log(&format!("---- lanciatore {} · {} ----", std::process::id(), exe.display()));
    avvio::log(&match net_framework() {
        Some(r) if r >= NET_462 => format!(".NET Framework ok (release {r}, richiesto 4.6.2 = {NET_462})"),
        Some(r) => format!(".NET Framework troppo vecchio (release {r}): il modulo di salvataggio richiede 4.6.2 = {NET_462}"),
        None => ".NET Framework 4.x non trovato: il modulo di salvataggio richiede 4.6.2".to_string(),
    });
    let mut modo = modo_grafico();
    let mut ultimo = String::new();
    for tentativo in 1..=MAX_TENTATIVI {
        let _ = std::fs::remove_file(file_finestra());
        let figlio = std::process::Command::new(&exe)
            .args(std::env::args().skip(1))
            .env("RD_FIGLIO", "1")
            .env("RD_GRAFICA", &modo)
            .env("RD_TENTATIVO", tentativo.to_string())
            .env_remove("RD_RIAVVIO")
            .spawn();
        let mut figlio = match figlio {
            Ok(f) => f,
            Err(e) => {
                ultimo = format!("avvio dell'app non riuscito: {e}");
                avvio::log(&format!("tentativo {tentativo}: {ultimo}"));
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }
        };
        let pid = figlio.id();
        let inizio = std::time::Instant::now();
        let limite = inizio + std::time::Duration::from_secs(ATTESA_FINESTRA);
        let mut aperta = false;
        let mut finito: Option<std::process::ExitStatus> = None;
        while std::time::Instant::now() < limite {
            if std::fs::read_to_string(file_finestra()).ok().and_then(|s| s.trim().parse::<u32>().ok()) == Some(pid) {
                aperta = true;
                break;
            }
            if let Ok(Some(s)) = figlio.try_wait() {
                finito = Some(s);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if aperta {
            if tentativo > 1 {
                avvio::log(&format!("finestra aperta al tentativo {tentativo} (grafica {modo})"));
            }
            // the app runs: keep the lock until it ends
            let fine = figlio.wait().ok().and_then(|s| s.code()).map(|c| c as u32).unwrap_or(0);
            if fine != 0 {
                avvio::log(&format!("app {pid} terminata dopo {} s: codice 0x{fine:08X} ({})", inizio.elapsed().as_secs(), spiega_codice(fine)));
            }
            return;
        }
        ultimo = match finito {
            Some(s) => {
                let c = s.code().map(|c| c as u32).unwrap_or(0);
                format!(
                    "app {pid} (grafica {modo}) chiusa senza finestra dopo {:.1} s: codice 0x{c:08X} ({})",
                    inizio.elapsed().as_secs_f32(),
                    spiega_codice(c)
                )
            }
            None => {
                // no frame in time: stuck (driver) — end it from outside
                let _ = figlio.kill();
                let _ = figlio.wait();
                format!("app {pid} (grafica {modo}) senza finestra dopo {ATTESA_FINESTRA} s: bloccata, chiusa dal lanciatore")
            }
        };
        let prossimo = modo_successivo(&modo);
        avvio::log(&format!("tentativo {tentativo} di {MAX_TENTATIVI}: {ultimo} → riprovo con grafica {prossimo}"));
        modo = prossimo.to_string();
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    avviso_windows(&format!(
        "Raccolta Dati non è riuscito ad aprire la finestra dopo {MAX_TENTATIVI} tentativi.\n\nUltimo: {ultimo}\n\nDettagli in {}",
        avvio::log_path().display()
    ));
}

fn avviso_windows(testo: &str) {
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (t, c) = (w(testo), w(TITOLO));
    // MB_OK | MB_ICONERROR
    unsafe { MessageBoxW(0, t.as_ptr(), c.as_ptr(), 0x10) };
}

fn main() {
    // Without a console nothing would show a crash: everything goes to avvio.log
    std::panic::set_hook(Box::new(|info| {
        let dove = info.location().map(|l| format!(" ({}:{})", l.file(), l.line())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        avvio::log(&format!("ERRORE GRAVE: {msg}{dove}"));
    }));
    let shot = arg_value(&std::env::args().collect::<Vec<_>>(), "--screenshot");
    // The operator's click: the launcher (test pictures run the app directly)
    if shot.is_none() && std::env::var("RD_FIGLIO").is_err() {
        lanciatore();
        return;
    }
    let tentativo: u32 = std::env::var("RD_TENTATIVO").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let modo = modo_grafico();
    let _ = MODO_ATTUALE.set(modo.clone());
    avvio::log(&format!("app {} avviata (tentativo {tentativo}, grafica {modo})", std::process::id()));
    if shot.is_some() {
        // test pictures: never remember a mode
        unsafe { std::env::set_var("RD_SCREENSHOT", "1") };
    }
    // Ends with a code the launcher reads: 0 closed, 2 window error, 3 program error
    let codice = match std::panic::catch_unwind(avvia) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            avvio::log(&format!("app {}: la finestra non si è aperta: {e}", std::process::id()));
            2
        }
        Err(_) => {
            avvio::log(&format!("app {}: fermata per un errore (vedi ERRORE GRAVE)", std::process::id()));
            3
        }
    };
    std::process::exit(codice);
}

fn avvia() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let shot = arg_value(&args, "--screenshot").map(PathBuf::from);
    // (the one-copy lock is held by the launcher)
    let ini = config::Ini::load(&config::ini_path(&args));
    let schermo_intero = shot.is_none() && ini.as_ref().map(|i| config::Postazione::from_ini(i).schermo_intero).unwrap_or(false);
    let opzioni = app::Opzioni {
        ini,
        shot: shot.clone(),
        sola_lettura: args.iter().any(|a| a == "--sola-lettura"),
        tab: arg_value(&args, "--scheda").and_then(|t| app::Tab::from_name(&t)),
        macchina: arg_value(&args, "--macchina"),
        versa: args.iter().any(|a| a == "--versa"),
        scansiona: arg_value(&args, "--scansiona"),
        barcode: args.iter().any(|a| a == "--barcode"),
        accesso: args.iter().any(|a| a == "--accesso"),
        operatore: arg_value(&args, "--operatore"),
        impostazioni: args.iter().any(|a| a == "--impostazioni"),
        info: arg_value(&args, "--info"),
        lotto: arg_value(&args, "--lotto").map(|l| l.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()).unwrap_or_default(),
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(TITOLO)
        .with_inner_size([1366.0, 800.0])
        .with_min_inner_size([1180.0, 720.0])
        .with_icon(icon())
        .with_active(true);
    if schermo_intero {
        viewport = viewport.with_fullscreen(true);
    } else if shot.is_none() {
        // The original opens maximised on the touch monitor
        // (ESP_FormMain::.ctor 0x1a054: WindowState = FormWindowState.Maximized)
        viewport = viewport.with_maximized(true);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // Graphics mode: one backend only, so a broken driver of another one is never touched
    match MODO_ATTUALE.get().map(|s| s.as_str()).unwrap_or("dx12") {
        "opengl" => options.renderer = eframe::Renderer::Glow,
        "software" => {
            use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
            options.renderer = eframe::Renderer::Wgpu;
            unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
            let mut setup = WgpuSetupCreateNew::without_display_handle();
            setup.instance_descriptor.backends = wgpu::Backends::DX12;
            setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters: &[wgpu::Adapter], _surface: Option<&wgpu::Surface<'_>>| {
                adapters
                    .iter()
                    .find(|a| a.get_info().device_type == wgpu::DeviceType::Cpu)
                    .cloned()
                    .ok_or_else(|| "nessun adattatore grafico software (WARP)".to_string())
            }));
            options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(setup);
        }
        m => {
            options.renderer = eframe::Renderer::Wgpu;
            // read by wgpu (Backends::from_env) when the window's renderer starts
            unsafe { std::env::set_var("WGPU_BACKEND", if m == "vulkan" { "vulkan" } else { "dx12" }) };
        }
    }
    // Test aid: RD_PROVA_GUASTO=n makes the attempts before n fail like a broken window opening
    if let Some(n) = std::env::var("RD_PROVA_GUASTO").ok().and_then(|v| v.parse::<u32>().ok()) {
        let t: u32 = std::env::var("RD_TENTATIVO").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        if t < n {
            return Err(eframe::Error::AppCreation(format!("guasto simulato (tentativo {t})").into()));
        }
    }
    // Test aid: RD_PROVA_CRASH=<mode> dies silently like on the work PC (no log line)
    if std::env::var("RD_PROVA_CRASH").ok().as_deref() == MODO_ATTUALE.get().map(|s| s.as_str()) {
        std::process::abort();
    }
    // Test aid: RD_PROVA_BLOCCO=<mode> hangs like the work PC's driver in that mode
    if std::env::var("RD_PROVA_BLOCCO").ok().as_deref() == MODO_ATTUALE.get().map(|s| s.as_str()) {
        avvio::log("apertura finestra… (blocco simulato)");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
    avvio::log("apertura finestra…");
    eframe::run_native(TITOLO, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, opzioni)))))
}
