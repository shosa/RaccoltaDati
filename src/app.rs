//! Main window: start-up (company), header, tab row, the screens, the bottom
//! action bar, dialogs and the quantity editor. Reads go straight to the
//! database; saves go through the helper that runs the original's own business
//! rules (ponte.rs).

mod dialoghi;
mod impostazioni;
mod lotto;
mod rapido;
mod schermi;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime};
use eframe::egui::{self, Align2, Color32, Margin, Rect, Stroke, Ui, pos2, vec2};

use crate::avvio::{self, Passi};
use crate::config::{Ini, Postazione};
use crate::dati::{self, Ambiente, Commessa, Fase, FiltroCommesse, Macchina, RigaArticolo, Setup, StatoFase, Turno, Versato};
use crate::db::{ConnParams, Db, Loader, Task, spawn_on};
use crate::ponte::{Ponte, Risposta};
use crate::theme::*;
use crate::ui::{self as w, Icon, Kind};

pub struct ShotState {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
    pub quiete: u32,
    pub image: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
}

pub struct Opzioni {
    pub ini: Result<Ini, String>,
    pub shot: Option<PathBuf>,
    pub sola_lettura: bool,
    pub tab: Option<Tab>,
    pub macchina: Option<String>,
    /// Test start options: open the quantity screen, then simulate one scan.
    pub versa: bool,
    pub scansiona: Option<String>,
    /// Test start option: open the barcode window (with --versa: the editor's).
    pub barcode: bool,
    /// Test start option: open the operator login keyboard.
    pub accesso: bool,
    /// Test start option, read-only runs only: start as if this operator had logged in.
    pub operatore: Option<String>,
    /// Test start option: open the settings editor.
    pub impostazioni: bool,
    /// Test start option: INFO COMMESSA lookup of this code (needs --operatore).
    pub info: Option<String>,
    /// Test start option: PIÙ COMMESSE list built from these codes (needs --operatore, --macchina).
    pub lotto: Vec<String>,
}

struct Startup {
    db: Db,
    aziende: Vec<(String, String)>,
}

struct Pronto {
    db: Db,
    az: String,
    az_nome: String,
    setup: Setup,
    macchine: Vec<Macchina>,
    ponte: Arc<Ponte>,
}

enum Stato {
    Avvio { task: Option<Task<Startup>>, startup: Option<Startup>, errore: Option<String>, pronto: Option<Task<Pronto>> },
    Main(Box<Main>),
}

pub struct App {
    rt: tokio::runtime::Runtime,
    ini: Result<Ini, String>,
    sola_lettura: bool,
    screenshot: bool,
    tab_iniziale: Option<Tab>,
    macchina_iniziale: Option<String>,
    prova_versa: bool,
    prova_scan: Option<String>,
    prova_barcode: bool,
    prova_accesso: bool,
    prova_operatore: Option<String>,
    prova_impostazioni: bool,
    prova_info: Option<String>,
    prova_lotto: Vec<String>,
    stato: Stato,
    shot: Option<ShotState>,
    passi: Passi,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, o: Opzioni) -> Self {
        apply(&cc.egui_ctx);
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().expect("tokio");
        let mut app = Self {
            rt,
            // The screenshot run never writes
            sola_lettura: o.sola_lettura || o.shot.is_some(),
            screenshot: o.shot.is_some(),
            passi: Passi::new(),
            tab_iniziale: o.tab,
            macchina_iniziale: o.macchina,
            prova_versa: o.versa,
            prova_scan: o.scansiona,
            prova_barcode: o.barcode,
            prova_accesso: o.accesso,
            prova_operatore: o.operatore,
            prova_impostazioni: o.impostazioni,
            prova_info: o.info,
            prova_lotto: o.lotto,
            stato: Stato::Avvio { task: None, startup: None, errore: None, pronto: None },
            shot: o.shot.map(|path| ShotState { path, frames: 0, requested: false, quiete: 0, image: Arc::new(Mutex::new(None)) }),
            ini: o.ini,
        };
        app.connetti(&cc.egui_ctx);
        app
    }

    /// Program.Main: connection data from Esprit.ini [Connect].
    fn connetti(&mut self, ctx: &egui::Context) {
        self.passi = Passi::new();
        avvio::log(&format!(
            "---- avvio Raccolta Dati (processo {} · {}) ----",
            std::process::id(),
            std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
        ));
        let passi = self.passi.clone();
        passi.inizia(avvio::INI, crate::config::ini_path(&std::env::args().collect::<Vec<_>>()).display().to_string());
        let ini = match &self.ini {
            Ok(i) => i.clone(),
            Err(e) => {
                passi.errore(avvio::INI, e.clone());
                self.stato = Stato::Avvio { task: None, startup: None, errore: Some(e.clone()), pronto: None };
                return;
            }
        };
        let params = ConnParams {
            server: ini.get("Connect", "SERVERSQL"),
            database: ini.get("Connect", "DATABASESQL"),
            user: ini.get("Connect", "UIDSQL"),
            password: ini.get("Connect", "PWDSQL"),
        };
        if params.server.trim().is_empty() {
            let e = "Server SQL non indicato in Esprit.ini ([Connect] SERVERSQL). Il collegamento tramite Web API non è ancora gestito.".to_string();
            passi.errore(avvio::INI, e.clone());
            self.stato = Stato::Avvio { task: None, startup: None, errore: Some(e), pronto: None };
            return;
        }
        passi.fatto_con(avvio::INI, format!("server {} · database {}", params.server, params.database));
        let rt = self.rt.handle().clone();
        let ctx2 = ctx.clone();
        let task = spawn_on(self.rt.handle(), ctx, async move {
            // 1. the server answers on its port (separates network problems from login problems)
            let (host, port) = params.host_port();
            passi.inizia(avvio::SERVER, format!("{host}:{port}"));
            match tokio::time::timeout(crate::db::CONNECT_TIMEOUT, tokio::net::TcpStream::connect((host.as_str(), port))).await {
                Ok(Ok(_)) => passi.fatto(avvio::SERVER),
                Ok(Err(e)) => {
                    let m = format!("Il server {host} non accetta connessioni sulla porta {port}: {e}");
                    passi.errore(avvio::SERVER, m.clone());
                    return Err(m);
                }
                Err(_) => {
                    let m = format!("Il server {host} non risponde entro {} s (rete, VPN o server spento?)", crate::db::CONNECT_TIMEOUT.as_secs());
                    passi.errore(avvio::SERVER, m.clone());
                    return Err(m);
                }
            }
            // 2. login + companies (the original's login check)
            passi.inizia(avvio::DATABASE, format!("{} · utente {}", params.database, params.user));
            let db = Db::new(params, rt, ctx2);
            match dati::aziende(&db).await {
                Ok(aziende) => {
                    passi.fatto_con(avvio::DATABASE, format!("{} azienda/e", aziende.len()));
                    Ok(Startup { db, aziende })
                }
                Err(e) => {
                    let m = format!("Accesso al database non riuscito: {e}");
                    passi.errore(avvio::DATABASE, m.clone());
                    Err(m)
                }
            }
        });
        self.stato = Stato::Avvio { task: Some(task), startup: None, errore: None, pronto: None };
    }

    fn crea_ponte(&self, az: &str) -> Arc<Ponte> {
        let ini = self.ini.as_ref().expect("ini");
        let post = Postazione::from_ini(ini);
        let lib_dir = std::env::var("RD_LIBDIR").unwrap_or_else(|_| r"C:\Esprit\Raccolta Dati".to_string());
        let yn = |b: bool| if b { "Y".to_string() } else { "N".to_string() };
        let init = vec![
            ("server".to_string(), ini.get("Connect", "SERVERSQL")),
            ("database".to_string(), ini.get("Connect", "DATABASESQL")),
            ("user".to_string(), ini.get("Connect", "UIDSQL")),
            ("password".to_string(), ini.get("Connect", "PWDSQL")),
            ("azienda".to_string(), az.to_string()),
            ("operatori".to_string(), post.operatori.join(",")),
            ("usa_badge".to_string(), yn(post.usa_badge)),
            ("torna_scheda".to_string(), yn(post.torna_scheda_macchine)),
        ];
        Ponte::new(Ponte::exe_path(), lib_dir, init, self.sola_lettura)
    }

    fn scegli_azienda(&mut self, az: String, nome: String) {
        let ponte = self.crea_ponte(&az);
        let salta_ponte = self.screenshot;
        let Stato::Avvio { startup, pronto, .. } = &mut self.stato else { return };
        let Some(s) = startup else { return };
        let db = s.db.clone();
        let codici = self.ini.as_ref().map(|i| Postazione::from_ini(i).macchine).unwrap_or_default();
        let passi = self.passi.clone();
        *pronto = Some(db.spawn({
            let db = db.clone();
            async move {
                passi.inizia(avvio::SETUP, format!("azienda {az}"));
                let setup = dati::load_setup(&db, &az).await;
                passi.fatto_con(avvio::SETUP, format!("decimali {} · ordine {}", setup.ndec, setup.ord_comme));
                passi.inizia(avvio::MACCHINE, if codici.is_empty() { "Esprit.ini: MACCHINE vuoto".to_string() } else { codici.join(", ") });
                let macchine = match dati::macchine(&db, &az, &codici).await {
                    Ok(m) => m,
                    Err(e) => {
                        passi.errore(avvio::MACCHINE, e.clone());
                        return Err(e);
                    }
                };
                if macchine.is_empty() {
                    passi.avviso(avvio::MACCHINE, "nessuna macchina trovata: controllare MACCHINE in Esprit.ini");
                } else {
                    passi.fatto_con(avvio::MACCHINE, macchine.iter().map(|m| m.cod.clone()).collect::<Vec<_>>().join(", "));
                }
                // The save helper: started now, so a problem shows here and the first tap is quick
                passi.inizia(avvio::PONTE, ponte.exe_display());
                if salta_ponte {
                    passi.fatto_con(avvio::PONTE, "non avviato (sola lettura)");
                } else if !ponte.available() {
                    passi.avviso(avvio::PONTE, "non trovato: si può consultare ma non registrare");
                } else {
                    let p2 = ponte.clone();
                    match tokio::task::spawn_blocking(move || p2.call("ping", &[])).await {
                        Ok(Risposta::Ok(_)) => passi.fatto(avvio::PONTE),
                        Ok(Risposta::Errore(e)) => passi.avviso(avvio::PONTE, e),
                        Ok(_) => passi.fatto(avvio::PONTE),
                        Err(e) => passi.avviso(avvio::PONTE, e.to_string()),
                    }
                }
                Ok(Pronto { db, az, az_nome: nome, setup, macchine, ponte })
            }
        }));
    }

    fn apri_main(&mut self, p: Pronto) {
        let ini = self.ini.as_ref().expect("ini");
        let post = Postazione::from_ini(ini);
        let ponte = p.ponte.clone();
        let mut m = Main::new(p, post, ponte, self.rt.handle().clone(), self.sola_lettura, self.passi.clone());
        m.ini_path = ini.path.clone();
        if let Some(c) = self.macchina_iniziale.clone() {
            if m.macchine.value.as_ref().map(|v| v.iter().any(|x| x.cod == c)).unwrap_or(false) {
                m.mac = Some(c);
            }
        }
        // never a way around the login when saving is possible
        if let (Some(op), true) = (self.prova_operatore.clone(), self.sola_lettura) {
            m.sel_op = Some(op.clone());
            m.accesso = Some((op, String::new()));
            m.tab = Tab::Macchine;
        }
        if let Some(t) = self.tab_iniziale {
            m.tab = t;
        }
        m.prova_versa = self.prova_versa;
        m.prova_scan = self.prova_scan.clone();
        m.prova_barcode = self.prova_barcode;
        m.avvia();
        if self.prova_accesso {
            m.inizia_turno();
        }
        if self.prova_impostazioni {
            m.apri_impostazioni();
        }
        if let Some(c) = self.prova_info.clone() {
            m.apri_info();
            m.barcode_letto(c);
        }
        if !self.prova_lotto.is_empty() && !m.bloccato() {
            let mut codici = self.prova_lotto.clone();
            let primo = codici.remove(0);
            m.prova_lotto = codici;
            m.apri_lotto();
            m.barcode_letto(primo);
        }
        self.stato = Stato::Main(Box::new(m));
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let busy = match &self.stato {
            Stato::Main(m) => m.occupato(),
            Stato::Avvio { errore, .. } => errore.is_none(),
        };
        let Some(s) = &mut self.shot else { return };
        s.frames += 1;
        s.quiete = if busy { 0 } else { s.quiete + 1 };
        ctx.request_repaint();
        // ~120 frames, then wait (bounded) for the data and ~30 still frames so the picture is settled
        if s.frames >= 120 && (s.quiete >= 30 || s.frames > 900) && !s.requested {
            s.requested = true;
            let slot = s.image.clone();
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                *slot.lock().unwrap() = Some(img);
                c2.request_repaint();
            });
        }
        let img = s.image.lock().unwrap().take();
        if let Some(img) = img {
            let [wd, ht] = img.size;
            let mut buf = Vec::with_capacity(wd * ht * 4);
            for p in &img.pixels {
                buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
            }
            if let Some(dir) = s.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Some(im) = image::RgbaImage::from_raw(wd as u32, ht as u32, buf) {
                if let Err(e) = im.save(&s.path) {
                    eprintln!("screenshot: {e}");
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if s.frames > 1500 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // the window draws: the graphics mode works (main.rs watchdog)
        crate::finestra_pronta();
        let ctx = ui.ctx().clone();
        self.screenshot_tick(&ctx);
        // Development aid: RD_ICONE=1 draws every icon large to check the drawing
        if std::env::var("RD_ICONE").is_ok() {
            egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| {
                let p = ui.painter();
                let all = [
                    Icon::Layers, Icon::Factory, Icon::Person, Icon::Play, Icon::Pause, Icon::Stop, Icon::Cube, Icon::Chart, Icon::Gear, Icon::Doc,
                    Icon::Filter, Icon::Up, Icon::Down, Icon::Refresh, Icon::Exit, Icon::Calendar, Icon::Tag, Icon::Clipboard, Icon::Clock,
                    Icon::Database, Icon::Check, Icon::Close, Icon::Plus, Icon::Key, Icon::Login, Icon::Logout, Icon::Pin, Icon::Backspace,
                ];
                for (i, ic) in all.iter().enumerate() {
                    let o = pos2(30.0 + (i % 7) as f32 * 190.0, 30.0 + (i / 7) as f32 * 190.0);
                    p.rect_filled(Rect::from_min_size(o, vec2(170.0, 170.0)), 12.0, CARD);
                    w::icon(p, Rect::from_min_size(o + vec2(15.0, 15.0), vec2(96.0, 96.0)), *ic, PRIMARY);
                    w::icon(p, Rect::from_min_size(o + vec2(120.0, 20.0), vec2(34.0, 34.0)), *ic, TEXT);
                    w::icon(p, Rect::from_min_size(o + vec2(124.0, 70.0), vec2(20.0, 20.0)), *ic, TEXT);
                    p.text(o + vec2(85.0, 150.0), Align2::CENTER_CENTER, format!("{ic:?}"), reg(15.0), MUTED);
                }
            });
            return;
        }

        let mut scelta: Option<(String, String)> = None;
        let mut riprova = false;
        let mut to_main: Option<Pronto> = None;
        if let Stato::Avvio { task, startup, errore, pronto } = &mut self.stato {
            if let Some(t) = task {
                if let Some(r) = t.poll() {
                    *task = None;
                    match r {
                        Ok(s) => {
                            if s.aziende.len() == 1 {
                                scelta = Some(s.aziende[0].clone());
                            } else if s.aziende.is_empty() {
                                *errore = Some("Nessuna azienda trovata nel database".into());
                            }
                            *startup = Some(s);
                        }
                        Err(e) => *errore = Some(e),
                    }
                }
            }
            if let Some(t) = pronto {
                if let Some(r) = t.poll() {
                    *pronto = None;
                    match r {
                        Ok(p) => to_main = Some(p),
                        Err(e) => *errore = Some(e),
                    }
                }
            }
        }
        if let Some((az, nome)) = scelta.take() {
            self.scegli_azienda(az, nome);
        }
        if let Some(p) = to_main {
            self.apri_main(p);
        }

        match &mut self.stato {
            Stato::Avvio { startup, .. } => {
                let aziende = startup.as_ref().map(|s| s.aziende.clone()).unwrap_or_default();
                let passi = self.passi.clone();
                egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| match avvio::splash(ui, &passi, &aziende, false) {
                    avvio::Scelta::Riprova => riprova = true,
                    avvio::Scelta::Azienda(a, n) => scelta = Some((a, n)),
                    _ => {}
                });
            }
            Stato::Main(m) => m.ui(ui),
        }
        if let Some((az, nome)) = scelta {
            self.scegli_azienda(az, nome);
        }
        if riprova {
            self.connetti(&ctx);
        }
    }
}

/// 3×3 tile mark (a neutral placeholder logo).
pub fn logo(p: &egui::Painter, at: egui::Pos2, size: f32) {
    let cell = size / 3.0;
    for i in 0..3 {
        for j in 0..3 {
            let r = Rect::from_min_size(at + vec2(j as f32 * cell, i as f32 * cell), vec2(cell - 3.0, cell - 3.0));
            let c = lerp(Color32::from_rgb(56, 132, 230), PRIMARY_DARK, (i + j) as f32 / 4.0);
            p.rect_filled(r, 3.0, c);
        }
    }
}

// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Commesse,
    Macchine,
    Operatori,
    Fasi,
    Produzione,
    Articoli,
}

impl Tab {
    pub fn from_name(s: &str) -> Option<Tab> {
        Some(match s.to_lowercase().as_str() {
            "commesse" => Tab::Commesse,
            "macchine" => Tab::Macchine,
            "operatori" => Tab::Operatori,
            "fasi" => Tab::Fasi,
            "produzione" | "pezzi" => Tab::Produzione,
            "articoli" => Tab::Articoli,
            _ => return None,
        })
    }
}

/// Main tabs, in the person's order. COMMESSE groups the order list and the
/// phases (two sub-tabs); the recorded quantities open from the phases.
/// The last slot of the row is the BARCODE button (no tab). The original's machine-data
/// tab (CaricaGridDatiMacchine 0x24ed0, read-only; its 2000 ms timer 0x25104 refreshes only
/// that tab) was removed on request, so nothing else depends on it.
const TABS: [(Tab, Icon, &str); 4] = [
    (Tab::Operatori, Icon::Person, "OPERATORI"),
    (Tab::Macchine, Icon::Factory, "MACCHINE"),
    (Tab::Commesse, Icon::Layers, "COMMESSE"),
    (Tab::Articoli, Icon::Tag, "ARTICOLI"),
];

impl Tab {
    /// The main tab a screen belongs to.
    fn gruppo(self) -> Tab {
        match self {
            Tab::Fasi | Tab::Produzione => Tab::Commesse,
            t => t,
        }
    }
}

/// What to do once a save answers.
#[derive(Clone, Debug)]
enum Dopo {
    Ricarica,
    Login { cod: String },
    TurnoIniziato { cod: String, nome: String },
    CommessaIniziata,
    FaseChiusa,
    VersApri,
    VersScritto,
    /// A step of the automatic barcode flow (rapido.rs).
    Rapido,
    /// PRENDI MACCHINA done by this operator on this PC.
    MacchinaPresa { cod: String },
    /// Shift ended with FINE TURNO.
    TurnoFinito { cod: String },
    /// Closing the app: shift of a session operator ended (machines released).
    Uscita { cod: String },
}

struct Azione {
    cmd: String,
    args: Vec<(String, String)>,
    dopo: Dopo,
    task: Task<Risposta>,
}

#[derive(Clone, Debug)]
enum Scopo {
    CodiceOperatore,
    Password { cod: String },
    FiltroCommessa,
    QtaRiga(usize),
    FiltroVersamento,
    /// Quantity typed in the barcode confirmation card (row index).
    QtaScansione(usize),
    /// Code typed by hand in the barcode window.
    CodiceBarcode,
    /// A value of the settings editor (row of the ini file).
    ValoreIni(usize),
    /// Quantity of a row of the PIÙ COMMESSE list.
    QtaLotto(usize),
}

enum Dialogo {
    Messaggio { testo: String, errore: bool },
    Domanda { testo: String, code: String, tre: bool, cmd: String, args: Vec<(String, String)>, dopo: Dopo },
    Chiusura { titolo: String, ore: f64, minuti: f64, operatori: f64, cmd: String, args: Vec<(String, String)>, dopo: Dopo },
    Tastierino { titolo: String, valore: String, password: bool, trattino: bool, decimali: bool, scopo: Scopo },
    Esci,
    /// Barcode: several orders match the scanned code; the operator picks one.
    ScanScelta { codice: String, indici: Vec<usize> },
    /// Barcode: one order chosen; quantity proposed, waiting for confirmation.
    ScanConferma { codice: String, idx: usize, qta: f64 },
    /// Automatic flow: several open orders match the scanned code.
    RapidoScelta { codice: String, trovate: Vec<dati::Trovata> },
    /// BARCODE button: the reader listens only while this window is open; it closes
    /// on a match, otherwise shows why and keeps listening.
    Barcode { errore: Option<String>, ricerca: bool, info: bool },
    /// PIÙ COMMESSE: scans verified and listed, written with AVVIA (lotto.rs).
    Lotto { voci: Vec<lotto::VoceLotto>, errore: Option<String>, ricerca: bool },
    /// PIÙ COMMESSE: several open orders match a scan; pick one to add.
    LottoScelta { codice: String, trovate: Vec<dati::Trovata> },
    /// INFO COMMESSA: what the scanned code belongs to (reads only).
    InfoCommessa { codice: String, trovate: Vec<dati::InfoCommessa> },
    /// Closing the app: ending a session operator's shift failed.
    UscitaErrore { testo: String },
}

/// Barcode match on the quantity rows: "105266", "20105266", "26-20105266" or
/// "2026-20105266". Exact number first, then numbers ending with the code, then
/// numbers containing it; a year part must match the end of the year.
fn corrispondenze(righe: &[RigaVers], codice: &str) -> Vec<usize> {
    let Some((anno, num)) = dati::analizza_codice(codice) else { return Vec::new() };
    let mut out: Vec<(u8, usize)> = righe
        .iter()
        .enumerate()
        .filter(|(_, r)| anno.as_ref().map(|a| r.anno.ends_with(a.as_str())).unwrap_or(true))
        .filter_map(|(i, r)| dati::rango(&r.num, &num).map(|g| (g, i)))
        .collect();
    out.sort();
    // the same order may appear on several rows (several items): keep all of them
    out.into_iter().map(|(_, i)| i).collect()
}

#[derive(Clone, Debug)]
struct RigaVers {
    anno: String,
    num: String,
    codart: String,
    descr: String,
    da_fare: f64,
    um: String,
    ndec: usize,
    rimanenti: f64,
    qta: Option<f64>,
}

struct Versamento {
    fase: String,
    operazione: String,
    ultima: bool,
    su_singole: bool,
    label: String,
    coeff: Option<f64>,
    versa_rimanenti: bool,
    solo_buoni: String,
    righe: Vec<RigaVers>,
    filtro: String,
    ultimo_codice: String,
    evidenzia: Option<usize>,
    /// Scroll the highlighted row into view once.
    scorri: bool,
    /// Opened by the automatic barcode flow: confirming the scan card writes at once.
    rapido: bool,
}

pub struct Main {
    db: Db,
    az: String,
    az_nome: String,
    setup: Setup,
    post: Postazione,
    ponte: Arc<Ponte>,
    rt: tokio::runtime::Handle,
    sola_lettura: bool,
    tab: Tab,
    macchine: Loader<Vec<Macchina>>,
    mac: Option<String>,
    amb: Loader<Ambiente>,
    turni: Loader<Vec<Turno>>,
    sel_op: Option<String>,
    commesse: Loader<Vec<Commessa>>,
    pagina: usize,
    /// Touch scrolling of the order and machine lists (▲/▼ move by a block/row).
    sc_comm: schermi::Scorri,
    sc_mac: schermi::Scorri,
    sel_comm: Option<(String, String, String)>,
    filtro: Option<(i32, i64)>,
    fasi: Loader<Vec<Fase>>,
    sel_fase: Option<i64>,
    versati: Loader<(bool, Vec<Versato>)>,
    articoli: Loader<Vec<RigaArticolo>>,
    azione: Option<Azione>,
    dialoghi: Vec<Dialogo>,
    avviso: Option<(String, Instant, bool)>,
    aggiornato: NaiveDateTime,
    vers: Option<Versamento>,
    passi: Passi,
    splash: bool,
    prova_versa: bool,
    prova_scan: Option<String>,
    prova_barcode: bool,
    /// Automatic barcode flow in progress, and the barcode window's reader buffer.
    rapido: Option<rapido::Rapido>,
    scan_buf: String,
    scan_ultimo: Option<Instant>,
    /// INFO COMMESSA lookup in progress (code, reads only).
    info: Option<(String, Task<Vec<dati::InfoCommessa>>)>,
    /// PIÙ COMMESSE: verification in progress, group orders still to run, finished orders.
    lotto_task: Option<Task<lotto::EsitoLotto>>,
    lotto_coda: Vec<Vec<lotto::VoceLotto>>,
    lotto_fatti: Vec<lotto::VoceLotto>,
    prova_lotto: Vec<String>,
    /// Operators who logged in or took a machine on this PC: closing the app ends their
    /// shift, which also takes them off their machines.
    sessione: Vec<String>,
    /// Operator logged in on this PC (code, name). Until then the app shows only the
    /// login card: no data, tabs or actions.
    accesso: Option<(String, String)>,
    /// Settings editor of Esprit.ini (login screen) and the file's path.
    impostazioni: Option<impostazioni::EditorIni>,
    ini_path: std::path::PathBuf,
    /// Full screen now (SCHERMO_INTERO at start, then the settings switch or F11).
    schermo_intero: bool,
    /// Closing in progress (shifts being ended), and close confirmed.
    uscita: bool,
    chiudi_ora: bool,
}

impl Main {
    fn new(p: Pronto, post: Postazione, ponte: Arc<Ponte>, rt: tokio::runtime::Handle, sola_lettura: bool, passi: Passi) -> Self {
        // No machine pre-selected (the original's constructor clicks Macchine[0],
        // ESP_FormMain::.ctor line 692): the operator logs in, then picks the machine.
        let mac = None;
        let schermo_intero = post.schermo_intero;
        let mut macchine = Loader::default();
        macchine.value = Some(p.macchine);
        Self {
            db: p.db,
            az: p.az,
            az_nome: p.az_nome,
            setup: p.setup,
            post,
            ponte,
            rt,
            sola_lettura,
            tab: Tab::Operatori,
            macchine,
            mac,
            amb: Loader::default(),
            turni: Loader::default(),
            sel_op: None,
            commesse: Loader::default(),
            pagina: 0,
            sc_comm: Default::default(),
            sc_mac: Default::default(),
            sel_comm: None,
            filtro: None,
            fasi: Loader::default(),
            sel_fase: None,
            versati: Loader::default(),
            articoli: Loader::default(),
            azione: None,
            dialoghi: Vec::new(),
            avviso: None,
            aggiornato: Local::now().naive_local(),
            vers: None,
            passi,
            splash: true,
            prova_versa: false,
            prova_scan: None,
            prova_barcode: false,
            rapido: None,
            scan_buf: String::new(),
            scan_ultimo: None,
            info: None,
            lotto_task: None,
            lotto_coda: Vec::new(),
            lotto_fatti: Vec::new(),
            prova_lotto: Vec::new(),
            sessione: Vec::new(),
            accesso: None,
            impostazioni: None,
            ini_path: std::path::PathBuf::from(crate::config::DEFAULT_INI),
            schermo_intero,
            uscita: false,
            chiudi_ora: false,
        }
    }

    fn bloccato(&self) -> bool {
        self.accesso.is_none()
    }

    fn schermo_intero(&mut self, ctx: &egui::Context, attivo: bool) {
        self.schermo_intero = attivo;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(attivo));
    }

    /// Operator logged in: unlock and go to the machines to take one.
    fn accedi(&mut self, cod: String, nome: String) {
        self.in_sessione(&cod);
        self.sel_op = Some(cod.clone());
        self.accesso = Some((cod, nome));
        self.tab = Tab::Macchine;
        self.carica_macchine();
        self.carica_turni();
    }

    /// The logged-in operator's shift ended: back to the login card.
    fn blocca(&mut self) {
        self.accesso = None;
        self.sel_op = None;
        self.vers = None;
        self.tab = Tab::Operatori;
    }

    fn in_sessione(&mut self, cod: &str) {
        if !self.sola_lettura && !self.sessione.iter().any(|c| c == cod) {
            self.sessione.push(cod.to_string());
        }
    }

    /// ESCI or the window's close button.
    fn chiedi_uscita(&mut self) {
        if !matches!(self.dialoghi.last(), Some(Dialogo::Esci)) && !self.uscita {
            self.dialoghi.push(Dialogo::Esci);
        }
    }

    /// Exit confirmed: end the shift of each session operator (FineTurnoOperatoreSelezionato:
    /// TurnoDal.FineTurno + StatoRDDal.TogliOperatoreDaMacchine), then close.
    fn prossima_uscita(&mut self) {
        match self.sessione.first().cloned() {
            Some(cod) => {
                self.uscita = true;
                self.esegui("turno_fine", vec![("cod", cod.clone())], Dopo::Uscita { cod });
            }
            None => self.chiudi_ora = true,
        }
    }

    fn occupato(&self) -> bool {
        self.splash
            || self.azione.is_some()
            || self.lotto_task.is_some()
            || !self.prova_lotto.is_empty()
            || self.info.is_some()
            || self.amb.loading()
            || self.commesse.loading()
            || self.fasi.loading()
            || self.turni.loading()
            || self.macchine.loading()
            || self.versati.loading()
    }

    fn macchina(&self) -> Option<Macchina> {
        let cod = self.mac.as_ref()?;
        self.macchine.value.as_ref()?.iter().find(|m| &m.cod == cod).cloned()
    }

    /// ESP_FormMain's constructor selects the first machine.
    fn avvia(&mut self) {
        if let Some(m) = self.mac.clone() {
            self.passi.inizia(avvio::DATI, format!("macchina {m}"));
            self.seleziona_macchina(m);
        } else {
            self.passi.inizia(avvio::DATI, "");
            self.passi.fatto_con(avvio::DATI, "pronto: accedi con l'operatore, poi scegli la macchina");
            self.splash = false;
            if let Some(c) = self.prova_scan.take() {
                self.apri_barcode();
                self.barcode_letto(c);
            } else if std::mem::take(&mut self.prova_barcode) {
                self.apri_barcode();
            }
            self.carica_turni();
        }
    }

    fn seleziona_macchina(&mut self, cod: String) {
        self.mac = Some(cod.clone());
        self.sel_comm = None;
        self.sel_fase = None;
        self.pagina = 0;
        self.sc_comm.inizio();
        // Macchine_ButtonClick: StatoRDDal.CreaSeInesistente (a write)
        if !self.sola_lettura && self.ponte.available() {
            let p = self.ponte.clone();
            std::thread::spawn(move || {
                let _ = p.call("macchina_seleziona", &[("mac", cod)]);
            });
        }
        self.ricarica_tutto();
    }

    fn ricarica_tutto(&mut self) {
        self.carica_macchine();
        self.carica_ambiente();
        self.carica_turni();
        self.aggiornato = Local::now().naive_local();
    }

    fn carica_macchine(&mut self) {
        let (db, az, codici) = (self.db.clone(), self.az.clone(), self.post.macchine.clone());
        self.macchine.start(self.db.spawn(async move { dati::macchine(&db, &az, &codici).await }));
    }

    fn carica_turni(&mut self) {
        let (db, az) = (self.db.clone(), self.az.clone());
        self.turni.start(self.db.spawn(async move { dati::turni(&db, &az).await }));
    }

    /// RefreshAmbiente; when it arrives the dependent lists reload.
    fn carica_ambiente(&mut self) {
        let Some(mac) = self.mac.clone() else { return };
        let (db, az) = (self.db.clone(), self.az.clone());
        self.amb.start(self.db.spawn(async move { dati::ambiente(&db, &az, &mac).await }));
    }

    fn carica_dipendenti(&mut self) {
        self.carica_commesse();
        self.carica_fasi();
        self.carica_versati();
        self.carica_articoli();
    }

    fn carica_commesse(&mut self) {
        let Some(mac) = self.macchina() else { return };
        let (db, az, setup, max_note) = (self.db.clone(), self.az.clone(), self.setup.clone(), self.post.max_len_note);
        let filtro = FiltroCommesse { anno: self.filtro.map(|f| f.0), num: self.filtro.map(|f| f.1) };
        self.commesse.start(self.db.spawn(async move { dati::commesse(&db, &az, &setup, &mac, &filtro, max_note).await }));
        self.aggiornato = Local::now().naive_local();
    }

    fn carica_fasi(&mut self) {
        let (Some(mac), Some(amb)) = (self.mac.clone(), self.amb.value.clone()) else { return };
        let (db, az, setup, ins) = (self.db.clone(), self.az.clone(), self.setup.clone(), self.post.ins_parziale);
        self.fasi.start(self.db.spawn(async move { dati::fasi(&db, &az, &setup, &amb, &mac, ins).await }));
    }

    fn carica_versati(&mut self) {
        let (Some(mac), Some(amb)) = (self.mac.clone(), self.amb.value.clone()) else { return };
        let (db, az, setup) = (self.db.clone(), self.az.clone(), self.setup.clone());
        self.versati.start(self.db.spawn(async move { dati::versati(&db, &az, &setup, &amb, &mac).await }));
    }

    fn carica_articoli(&mut self) {
        let Some(amb) = self.amb.value.clone() else { return };
        let (db, az) = (self.db.clone(), self.az.clone());
        self.articoli.start(self.db.spawn(async move { dati::articoli_commessa(&db, &az, &amb).await }));
    }

    fn avvisa(&mut self, t: impl Into<String>, errore: bool) {
        self.avviso = Some((t.into(), Instant::now(), errore));
    }

    fn messaggio(&mut self, t: impl Into<String>, errore: bool) {
        self.dialoghi.push(Dialogo::Messaggio { testo: t.into(), errore });
    }

    // ------------------------------------------------------------ saving

    fn esegui(&mut self, cmd: &str, args: Vec<(&str, String)>, dopo: Dopo) {
        let args: Vec<(String, String)> = args.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        self.esegui_owned(cmd.to_string(), args, dopo);
    }

    fn esegui_owned(&mut self, cmd: String, args: Vec<(String, String)>, dopo: Dopo) {
        if self.azione.is_some() {
            return;
        }
        if !self.ponte.available() {
            self.messaggio("Il modulo di salvataggio non è disponibile su questo computer (serve la cartella del programma originale).", true);
            return;
        }
        let p = self.ponte.clone();
        let (c2, a2) = (cmd.clone(), args.clone());
        let task = spawn_on(&self.rt, &self.db.ctx(), async move {
            tokio::task::spawn_blocking(move || {
                let refs: Vec<(&str, String)> = a2.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                p.call(&c2, &refs)
            })
            .await
            .map_err(|e| e.to_string())
        });
        self.azione = Some(Azione { cmd, args, dopo, task });
    }

    fn poll_azione(&mut self) {
        let Some(a) = &self.azione else { return };
        let Some(r) = a.task.poll() else { return };
        let a = self.azione.take().unwrap();
        let r = r.unwrap_or_else(Risposta::Errore);
        match r {
            Risposta::Errore(e) if self.rapido.is_some() && matches!(a.dopo, Dopo::Rapido | Dopo::VersApri | Dopo::VersScritto) => {
                self.rapido_errore(e);
            }
            Risposta::Errore(e) if matches!(a.dopo, Dopo::Uscita { .. }) => {
                self.uscita = false;
                self.dialoghi.push(Dialogo::UscitaErrore { testo: e });
                self.ricarica_tutto();
            }
            Risposta::Errore(e) => {
                self.messaggio(e, true);
                if matches!(a.dopo, Dopo::Ricarica | Dopo::FaseChiusa | Dopo::CommessaIniziata | Dopo::VersScritto | Dopo::MacchinaPresa { .. } | Dopo::TurnoFinito { .. }) {
                    self.ricarica_tutto();
                }
            }
            // The flow suspends the phase it started: the close window's proposal
            // (worked hours/minutes, operators) is accepted as is.
            Risposta::Finestra(m) if matches!(a.dopo, Dopo::Rapido) => {
                let mut args = a.args;
                args.retain(|(k, _)| !matches!(k.as_str(), "dtfine" | "ore" | "minuti" | "operatori"));
                for k in ["dtfine", "ore", "minuti", "operatori"] {
                    args.push((k.into(), m.get(k).cloned().unwrap_or_default()));
                }
                self.esegui_owned(a.cmd, args, a.dopo);
            }
            Risposta::Domanda { code, testo } => {
                self.dialoghi.push(Dialogo::Domanda { testo, tre: code == "189", code, cmd: a.cmd, args: a.args, dopo: a.dopo });
            }
            Risposta::Finestra(m) => {
                let num = |k: &str| m.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let mut args = a.args;
                args.retain(|(k, _)| k != "dtfine");
                args.push(("dtfine".into(), m.get("dtfine").cloned().unwrap_or_default()));
                self.dialoghi.push(Dialogo::Chiusura {
                    titolo: m.get("titolo").cloned().unwrap_or_default(),
                    ore: num("ore"),
                    minuti: num("minuti"),
                    operatori: num("operatori"),
                    cmd: a.cmd,
                    args,
                    dopo: a.dopo,
                });
            }
            Risposta::Ok(m) => self.dopo_ok(a, m),
        }
    }

    fn dopo_ok(&mut self, a: Azione, m: HashMap<String, String>) {
        let msg = m.get("msg").cloned().unwrap_or_default();
        let flag = |k: &str| m.get(k).map(|v| v == "Y").unwrap_or(false);
        if let Some(info) = m.get("info") {
            self.messaggio(info.clone(), false);
        }
        if let Some(av) = m.get("avviso") {
            self.messaggio(av.clone(), true);
        }
        match a.dopo {
            Dopo::Login { cod } => {
                if flag("serve_password") {
                    self.dialoghi.push(Dialogo::Tastierino {
                        titolo: "Password Operatore".into(),
                        valore: String::new(),
                        password: true,
                        trattino: false,
                        decimali: false,
                        scopo: Scopo::Password { cod },
                    });
                } else if flag("ok") {
                    let nome = m.get("nome").cloned().unwrap_or_default();
                    if flag("turno_aperto") {
                        // The original warns "Operatore già collegato" and selects the operator;
                        // here the shift goes on and the operator is let in (the app is locked
                        // until a login, e.g. after a restart).
                        self.avvisa(format!("Operatore già in turno: accesso eseguito ({})", if nome.is_empty() { &cod } else { &nome }), false);
                        self.accedi(cod, nome);
                    } else {
                        self.esegui("turno_inizia", vec![("cod", cod.clone())], Dopo::TurnoIniziato { cod, nome });
                    }
                }
            }
            Dopo::TurnoIniziato { cod, nome } => {
                self.avvisa("Turno iniziato", false);
                self.accedi(cod, nome);
                if self.post.torna_scheda_macchine && self.mac.is_some() {
                    self.controlla_macchina();
                }
            }
            Dopo::CommessaIniziata => {
                self.avvisa(if msg.is_empty() { "Commessa avviata sulla macchina".to_string() } else { msg }, false);
                self.ricarica_tutto();
                // IniziaCommessa_Click: without a running phase the original opens the phases
                self.tab = Tab::Fasi;
            }
            Dopo::FaseChiusa => {
                if !msg.is_empty() {
                    self.avvisa(msg, false);
                }
                if flag("commessa_terminata") {
                    self.messaggio("Commessa terminata.", false);
                }
                self.sel_fase = None;
                self.ricarica_tutto();
                if self.post.torna_scheda_macchine {
                    self.tab = Tab::Macchine;
                }
            }
            Dopo::VersApri => {
                self.apri_versamento(&m);
                if let Some(c) = self.prova_scan.take() {
                    self.apri_barcode();
                    self.barcode_letto(c);
                } else if std::mem::take(&mut self.prova_barcode) {
                    self.apri_barcode();
                }
                self.vers_aperto_rapido();
            }
            Dopo::VersScritto if self.rapido.is_some() => {
                if let Some(r) = &mut self.rapido {
                    r.registrato = true;
                }
                self.ripristina_rapido(None);
            }
            Dopo::Rapido => self.rapido_ok(flag("fatto")),
            Dopo::MacchinaPresa { cod } => {
                self.in_sessione(&cod);
                if !msg.is_empty() {
                    self.avvisa(msg, false);
                }
                self.ricarica_tutto();
            }
            Dopo::TurnoFinito { cod } => {
                self.sessione.retain(|c| c != &cod);
                if self.accesso.as_ref().map(|a| a.0 == cod).unwrap_or(false) {
                    self.blocca();
                }
                if !msg.is_empty() {
                    self.avvisa(msg, false);
                }
                self.ricarica_tutto();
            }
            Dopo::Uscita { cod } => {
                self.sessione.retain(|c| c != &cod);
                self.prossima_uscita();
            }
            Dopo::VersScritto => {
                self.vers = None;
                self.avvisa(if msg.is_empty() { "Quantità registrata".to_string() } else { msg }, false);
                // As the original: after ESP_FormPezziRag only the recorded pieces reload
                // (MostraMascheraVersamento 0x2228c -> RefreshPezzi 0x22420) — one query
                // instead of the whole screen, which matters on a slow network.
                self.carica_versati();
            }
            Dopo::Ricarica => {
                if !msg.is_empty() {
                    self.avvisa(msg, false);
                }
                self.ricarica_tutto();
            }
        }
    }

    fn rispondi(&mut self, cmd: String, mut args: Vec<(String, String)>, code: &str, risposta: &str, dopo: Dopo) {
        args.push((format!("r{code}"), risposta.to_string()));
        self.esegui_owned(cmd, args, dopo);
    }

    // ------------------------------------------------------------ actions

    fn inizia_turno(&mut self) {
        self.dialoghi.push(Dialogo::Tastierino {
            titolo: "Codice Operatore".into(),
            valore: String::new(),
            password: false,
            trattino: false,
            decimali: false,
            scopo: Scopo::CodiceOperatore,
        });
    }

    fn fine_turno(&mut self) {
        let Some(cod) = self.sel_op.clone() else {
            self.messaggio("Selezionare un operatore", true);
            return;
        };
        self.esegui("turno_fine", vec![("cod", cod.clone())], Dopo::TurnoFinito { cod });
        self.sel_op = None;
    }

    /// bControllaMacchina_Click, from the MACCHINE screen: the logged-in operator takes the
    /// selected machine.
    fn controlla_macchina(&mut self) {
        let Some(cod) = self.accesso.as_ref().map(|a| a.0.clone()).or_else(|| self.sel_op.clone()) else {
            self.messaggio("Selezionare un operatore", true);
            return;
        };
        let Some(mac) = self.mac.clone() else {
            self.messaggio("Selezionare una macchina", true);
            return;
        };
        self.esegui("macchina_controlla", vec![("mac", mac), ("cod", cod.clone())], Dopo::MacchinaPresa { cod });
    }

    fn commessa_selezionata(&self) -> Option<Commessa> {
        let k = self.sel_comm.as_ref()?;
        self.commesse.value.as_ref()?.iter().find(|c| &c.chiave() == k).cloned()
    }

    fn inizia_commessa(&mut self) {
        let Some(c) = self.commessa_selezionata() else {
            self.messaggio("Selezionare una commessa", true);
            return;
        };
        let mac = self.mac.clone().unwrap_or_default();
        self.esegui(
            "commessa_inizia",
            vec![("mac", mac), ("anno", c.anno), ("num", c.num), ("tipo", c.tipo), ("semaforo", c.semaforo.codice().to_string())],
            Dopo::CommessaIniziata,
        );
    }

    fn sospendi_commessa(&mut self) {
        let mac = self.mac.clone().unwrap_or_default();
        self.esegui("commessa_sospendi", vec![("mac", mac)], Dopo::Ricarica);
    }

    fn fase_selezionata(&self) -> Option<Fase> {
        let f = self.fasi.value.as_ref()?;
        let p = self.sel_fase?;
        f.iter().find(|x| x.progr == p).cloned()
    }

    fn inizia_fase(&mut self) {
        let Some(f) = self.fase_selezionata() else {
            self.messaggio("Selezionare una fase", true);
            return;
        };
        let mac = self.mac.clone().unwrap_or_default();
        self.esegui("fase_inizia", vec![("mac", mac), ("progr", f.progr.to_string()), ("semaforo", f.semaforo.codice().to_string())], Dopo::Ricarica);
    }

    /// ChiudiSospendiFase: "T" termina, "R" sospendi.
    fn chiudi_fase(&mut self, funz: &str, progr: Option<i64>) {
        let Some(progr) = progr.or(self.sel_fase) else {
            self.messaggio("Selezionare una fase", true);
            return;
        };
        let mac = self.mac.clone().unwrap_or_default();
        self.esegui("fase_chiudi", vec![("mac", mac), ("progr", progr.to_string()), ("funz", funz.to_string())], Dopo::FaseChiusa);
    }

    fn utilizza_fase(&mut self) {
        let Some(f) = self.fase_selezionata() else {
            self.messaggio("Selezionare una fase", true);
            return;
        };
        let mac = self.mac.clone().unwrap_or_default();
        self.esegui("fase_utilizza", vec![("mac", mac), ("progr", f.progr.to_string())], Dopo::Ricarica);
    }

    fn args_versamento(&self) -> Vec<(&'static str, String)> {
        vec![
            ("mac", self.mac.clone().unwrap_or_default()),
            ("ins_parziale", if self.post.ins_parziale { "S".into() } else { "N".into() }),
            ("registra_pesate", if self.post.registra_pesate { "Y".into() } else { "N".into() }),
        ]
    }

    /// bNewPezzi_Click: the editor opens with the rows the original's form loads
    /// (loading only reads, so it also works in read-only mode).
    fn versa_pezzi(&mut self) {
        let args = self.args_versamento();
        self.esegui("vers_carica", args, Dopo::VersApri);
    }

    fn apri_versamento(&mut self, m: &HashMap<String, String>) {
        let num = |s: &str| s.parse::<f64>().unwrap_or(0.0);
        let ndec = self.setup.ndec;
        let righe = m
            .get("righe")
            .map(|r| {
                r.split('\u{1e}')
                    .filter(|x| !x.is_empty())
                    .map(|row| {
                        let c: Vec<&str> = row.split('\u{1f}').collect();
                        let g = |i: usize| c.get(i).copied().unwrap_or("").to_string();
                        RigaVers {
                            anno: g(0),
                            num: g(1),
                            codart: g(3),
                            descr: g(4),
                            da_fare: num(&g(5)),
                            um: g(6),
                            ndec: g(7).parse().unwrap_or(ndec),
                            rimanenti: num(&g(9)),
                            qta: None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let flag = |k: &str| m.get(k).map(|v| v == "Y").unwrap_or(false);
        self.vers = Some(Versamento {
            fase: m.get("fase").cloned().unwrap_or_default(),
            operazione: m.get("operazione").cloned().unwrap_or_default(),
            ultima: flag("ultima"),
            su_singole: flag("su_singole"),
            label: m.get("label").cloned().unwrap_or_default(),
            coeff: m.get("coeff").and_then(|v| v.parse().ok()),
            versa_rimanenti: flag("versa_rimanenti"),
            solo_buoni: m.get("solo_buoni").cloned().unwrap_or_default(),
            righe,
            filtro: String::new(),
            ultimo_codice: String::new(),
            evidenzia: None,
            scorri: false,
            rapido: false,
        });
    }

    fn conferma_versamento(&mut self) {
        let Some(v) = &self.vers else { return };
        let righe: Vec<String> = v
            .righe
            .iter()
            .filter_map(|r| r.qta.filter(|q| *q > 0.0).map(|q| format!("{}\u{1f}{}\u{1f}{}\u{1f}{}", r.anno, r.num, r.codart, q)))
            .collect();
        if righe.is_empty() {
            self.messaggio("Indicare la quantità da versare", true);
            return;
        }
        let mut args = self.args_versamento();
        args.push(("righe", righe.join("\u{1e}")));
        self.esegui("vers_conferma", args, Dopo::VersScritto);
    }

    // ------------------------------------------------------------ frame

    fn poll(&mut self) {
        self.poll_azione();
        if self.macchine.poll() {
            if let Some(e) = self.macchine.error.clone() {
                self.avvisa(e, true);
            }
        }
        if self.amb.poll() {
            if let Some(e) = self.amb.error.clone() {
                if self.splash {
                    self.passi.errore(avvio::DATI, format!("stato macchina: {e}"));
                }
                self.avvisa(e, true);
            }
            self.carica_dipendenti();
        }
        if self.turni.poll() {
            if let Some(e) = self.turni.error.clone() {
                self.avvisa(e, true);
            }
        }
        if self.commesse.poll() {
            if let Some(e) = self.commesse.error.clone() {
                if self.splash {
                    self.passi.errore(avvio::DATI, e.clone());
                }
                self.avvisa(e, true);
            } else if self.splash {
                let n = self.commesse.value.as_ref().map(|v| v.len()).unwrap_or(0);
                self.passi.fatto_con(avvio::DATI, format!("{n} commesse · {:.1} s dall'avvio", self.passi.avviato.elapsed().as_secs_f32()));
                self.splash = false;
                if std::mem::take(&mut self.prova_versa) {
                    self.versa_pezzi();
                } else if let Some(c) = self.prova_scan.take() {
                    // test option: a scan in the main screen's barcode window (automatic flow)
                    self.apri_barcode();
                    self.barcode_letto(c);
                } else if std::mem::take(&mut self.prova_barcode) {
                    self.apri_barcode();
                }
            }
            // Checked = the order on the machine
            if let (Some(a), Some(list)) = (&self.amb.value, &self.commesse.value) {
                let k = (a.anno.clone(), a.num.clone(), a.tipo.clone());
                if self.sel_comm.is_none() && list.iter().any(|c| c.chiave() == k) {
                    self.sel_comm = Some(k);
                }
            }
        }
        if self.fasi.poll() {
            if let Some(e) = self.fasi.error.clone() {
                self.avvisa(e, true);
            }
            if let Some(list) = &self.fasi.value {
                let valid = self.sel_fase.map(|p| list.iter().any(|f| f.progr == p)).unwrap_or(false);
                if !valid {
                    // Checked = the phase in use on the machine
                    let used = self.amb.value.as_ref().and_then(|a| a.progr.parse::<i64>().ok());
                    self.sel_fase = used
                        .filter(|p| list.iter().any(|f| f.progr == *p))
                        .or_else(|| list.iter().find(|f| f.stato != StatoFase::Terminata).map(|f| f.progr));
                }
            }
        }
        self.versati.poll();
        self.articoli.poll();
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.poll();
        self.poll_rapido();
        self.poll_info();
        self.poll_lotto();
        self.ascolta_barcode(ui.ctx());
        let ctx = ui.ctx().clone();
        // Window close button: same as ESCI (session operators are logged out first)
        if ctx.input(|i| i.viewport().close_requested()) && !self.chiudi_ora && !self.sessione.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.chiedi_uscita();
        }
        if self.chiudi_ora {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // F11: full screen on/off for this session (the settings switch makes it permanent)
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            let on = !self.schermo_intero;
            self.schermo_intero(&ctx, on);
        }
        ctx.request_repaint_after(Duration::from_secs(1));

        if self.bloccato() {
            // Before the operator login: logo, exit and the login card only
            egui::Panel::top("header").exact_size(92.0).frame(egui::Frame::new().fill(HEADER)).show(ui, |ui| self.header(ui));
            egui::Panel::bottom("status").exact_size(40.0).frame(egui::Frame::new().fill(STATUS)).show(ui, |ui| self.status(ui));
            egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| self.schermo_accesso(ui));
        } else {
            self.schermo_principale(ui);
        }
        if self.impostazioni.is_some() {
            self.editor_impostazioni(&ctx);
        }
        self.dialoghi_ui(&ctx);
        if self.azione.is_some() {
            dialoghi::occupato_ui(&ctx);
        }
        self.avviso_ui(&ctx);
        if self.splash {
            let passi = self.passi.clone();
            let screen = ctx.content_rect();
            let mut scelta = avvio::Scelta::Niente;
            egui::Area::new(egui::Id::new("splash")).fade_in(false).order(egui::Order::Foreground).fixed_pos(screen.min).show(&ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.set_max_size(screen.size());
                scelta = avvio::splash(ui, &passi, &[], true);
            });
            match scelta {
                avvio::Scelta::Continua => self.splash = false,
                avvio::Scelta::Riprova => {
                    if let Some(m) = self.mac.clone() {
                        self.passi.inizia(avvio::DATI, format!("macchina {m}"));
                    }
                    self.ricarica_tutto();
                }
                _ => {}
            }
        }
    }

    /// Login card shown until an operator logs in on this PC.
    fn schermo_accesso(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let card = Rect::from_center_size(r.center(), vec2(620.0, 420.0));
        let p = ui.painter().clone();
        w::shadow(&p, card, 18, 30);
        p.rect_filled(card, 18.0, CARD);
        p.rect_stroke(card, 18.0, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
        let c = pos2(card.center().x, card.top() + 92.0);
        p.circle_filled(c, 52.0, lerp(PRIMARY, Color32::WHITE, 0.88));
        w::icon(&p, Rect::from_center_size(c, vec2(56.0, 56.0)), Icon::Person, PRIMARY_DARK);
        p.text(pos2(card.center().x, card.top() + 176.0), Align2::CENTER_CENTER, "Accesso operatore", bold(30.0), TEXT);
        p.text(pos2(card.center().x, card.top() + 214.0), Align2::CENTER_CENTER, "Inserisci il tuo codice operatore per iniziare il turno.", reg(18.0), MUTED);
        p.text(pos2(card.center().x, card.top() + 240.0), Align2::CENTER_CENTER, "Se il turno è già aperto, l'accesso lo riprende.", reg(16.0), FAINT);
        let br = Rect::from_center_size(pos2(card.center().x, card.bottom() - 82.0), vec2(360.0, 92.0));
        let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(br));
        if w::tile(&mut bui, br.size(), Some(Icon::Login), "ACCEDI", Kind::Good, self.azione.is_none()).clicked() {
            self.inizia_turno();
        }
        // Settings of the workstation (Esprit.ini), bottom right, no password
        let gr = Rect::from_min_size(r.right_bottom() - vec2(22.0 + 200.0, 18.0 + 76.0), vec2(200.0, 76.0));
        let mut gui = ui.new_child(egui::UiBuilder::new().max_rect(gr));
        if w::tile(&mut gui, gr.size(), Some(Icon::Gear), "IMPOSTAZIONI", Kind::Normal, true).clicked() {
            self.apri_impostazioni();
        }
    }

    fn schermo_principale(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("header").exact_size(92.0).frame(egui::Frame::new().fill(HEADER)).show(ui, |ui| self.header(ui));
        egui::Panel::bottom("status").exact_size(40.0).frame(egui::Frame::new().fill(STATUS)).show(ui, |ui| self.status(ui));
        egui::Panel::bottom("azioni")
            .exact_size(104.0)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 22, right: 22, top: 10, bottom: 14 }))
            .show(ui, |ui| self.barra_azioni(ui));
        egui::Panel::top("tabs")
            .exact_size(102.0)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 22, right: 22, top: 14, bottom: 10 }))
            .show(ui, |ui| self.tabs(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 22, right: 22, top: 6, bottom: 4 }))
            .show(ui, |ui| {
                if self.tab.gruppo() == Tab::Commesse {
                    self.sotto_schede(ui);
                }
                match self.tab {
                Tab::Commesse => self.vista_commesse(ui),
                Tab::Macchine => self.vista_macchine(ui),
                Tab::Operatori => self.vista_operatori(ui),
                Tab::Fasi => self.vista_fasi(ui),
                Tab::Produzione => self.vista_produzione(ui),
                Tab::Articoli => self.vista_articoli(ui),
                }
            });

        if self.vers.is_some() {
            self.editor_versamento(&ctx);
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, BORDER));
        // Company logo in place of the app mark and the "Raccolta Dati · MES" texts
        w::logo_aziendale(&p, Rect::from_min_size(pos2(r.left() + 20.0, r.center().y - 24.0), vec2(250.0, 48.0)));
        let exit_w = 84.0;
        let exit = Rect::from_min_size(pos2(r.right() - exit_w - 18.0, r.center().y - 34.0), vec2(exit_w, 68.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(exit));
        if w::tile(&mut child, exit.size(), Some(Icon::Exit), "", Kind::Dark, true).clicked() {
            self.chiedi_uscita();
        }
        if self.bloccato() {
            return;
        }
        let x0 = r.left() + 300.0;
        w::vsep(&p, x0 - 14.0, r.top() + 18.0, r.bottom() - 18.0);
        let amb = self.amb.value.clone().unwrap_or_default();
        let mac = self.macchina().map(|m| m.descr).unwrap_or_default();
        let blocks = [
            (Icon::Clipboard, "COMMESSA", amb.commessa_label()),
            (Icon::Person, "OPERATORE", amb.operatore.clone()),
            (Icon::Factory, "MACCHINA", if amb.macchina.is_empty() { mac } else { amb.macchina.clone() }),
            (Icon::Gear, "FASE", amb.fase.clone()),
        ];
        let pesi = [0.95, 1.2, 1.15, 0.7];
        let unit = (r.right() - exit_w - 30.0 - x0) / pesi.iter().sum::<f32>();
        let mut bx = x0;
        for (i, (ic, l, v)) in blocks.iter().enumerate() {
            let bw = unit * pesi[i];
            if i > 0 {
                w::vsep(&p, bx - 8.0, r.top() + 18.0, r.bottom() - 18.0);
            }
            w::icon(&p, Rect::from_min_size(pos2(bx + 4.0, r.center().y - 17.0), vec2(34.0, 34.0)), *ic, PRIMARY);
            let val = if v.trim().is_empty() { "—".to_string() } else { v.clone() };
            w::label_value(&p, pos2(bx + 48.0, r.center().y - 23.0), l, &val, 21.0, TEXT, bw - 60.0);
            bx += bw;
        }
    }

    fn tabs(&mut self, ui: &mut Ui) {
        // tabs + BARCODE share the row; INFO COMMESSA is a narrow icon-only button
        let n = TABS.len() as f32 + 1.0;
        let gap = 10.0;
        let info_w = 104.0;
        let wd = (ui.available_width() - info_w - gap * n) / n;
        let h = ui.available_height();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // BARCODE: opens the listening window (the reader is off otherwise)
                let libero = self.rapido.is_none() && self.azione.is_none();
                if w::tile(ui, vec2(wd, h), Some(Icon::Barcode), "BARCODE", Kind::Orange, libero).clicked() {
                    self.apri_barcode();
                }
                // INFO COMMESSA: scan and see which order it is, nothing written
                if w::tile(ui, vec2(info_w, h), Some(Icon::Search), "", Kind::Info, self.azione.is_none()).clicked() {
                    self.apri_info();
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| self.schede(ui, wd, h));
            });
        });
    }

    fn schede(&mut self, ui: &mut Ui, wd: f32, h: f32) {
        for (t, ic, l) in TABS {
            if w::tab(ui, vec2(wd, h), ic, l, self.tab.gruppo() == t).clicked() && self.tab.gruppo() != t {
                self.tab = t;
                // tabControl1_Selected: each tab refreshes its data
                match t {
                    Tab::Macchine => self.carica_macchine(),
                    Tab::Operatori => self.carica_turni(),
                    Tab::Commesse => {
                        if !self.post.auto_refresh_commesse {
                            self.carica_commesse();
                        }
                    }
                    Tab::Fasi => self.carica_ambiente(),
                    Tab::Produzione => self.carica_versati(),
                    Tab::Articoli => self.carica_articoli(),
                }
            }
        }
    }

    fn status(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let p = ui.painter();
        if self.bloccato() {
            // no company / database details before the login: only the clock
            let tr = p.text(pos2(r.right() - 22.0, r.center().y), Align2::RIGHT_CENTER, Local::now().format("%H:%M:%S").to_string(), reg(15.0), Color32::WHITE);
            w::icon(p, Rect::from_min_size(pos2(tr.left() - 28.0, r.center().y - 10.0), vec2(20.0, 20.0)), Icon::Clock, Color32::WHITE);
            if self.sola_lettura {
                p.text(r.center(), Align2::CENTER_CENTER, "SOLA LETTURA · nessuna registrazione viene salvata", bold(14.0), SEM_GIALLO);
            }
            return;
        }
        w::icon(p, Rect::from_min_size(pos2(r.left() + 22.0, r.center().y - 10.0), vec2(20.0, 20.0)), Icon::Database, Color32::WHITE);
        let db = &self.db.params().database;
        p.text(pos2(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, format!("DB: {}   ·   AZ: {}", db.trim(), self.az), reg(15.0), Color32::WHITE);
        if self.sola_lettura {
            p.text(r.center(), Align2::CENTER_CENTER, "SOLA LETTURA · nessuna registrazione viene salvata", bold(14.0), SEM_GIALLO);
        } else if let Some(rp) = &self.rapido {
            p.text(r.center(), Align2::CENTER_CENTER, rp.descrizione(), bold(15.0), SEM_GIALLO);
        } else if !self.ponte.available() {
            p.text(r.center(), Align2::CENTER_CENTER, "Modulo di salvataggio non trovato", bold(14.0), SEM_GIALLO);
        }
        let now = Local::now();
        let txt = format!("Ultimo aggiornamento:  {}     {}", self.aggiornato.format("%d/%m/%Y %H:%M"), now.format("%H:%M:%S"));
        let tr = p.text(pos2(r.right() - 22.0, r.center().y), Align2::RIGHT_CENTER, txt, reg(15.0), Color32::WHITE);
        w::icon(p, Rect::from_min_size(pos2(tr.left() - 28.0, r.center().y - 10.0), vec2(20.0, 20.0)), Icon::Clock, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riga(anno: &str, num: &str) -> RigaVers {
        RigaVers { anno: anno.into(), num: num.into(), codart: "A".into(), descr: String::new(), da_fare: 10.0, um: String::new(), ndec: 0, rimanenti: 4.0, qta: None }
    }

    #[test]
    fn barcode_matches() {
        let r = vec![riga("2026", "20105266"), riga("2026", "20105267"), riga("2025", "30105266"), riga("2026", "105266")];
        // exact first, then numbers ending with the code
        assert_eq!(corrispondenze(&r, "105266"), vec![3, 0, 2]);
        // year part narrows the match
        assert_eq!(corrispondenze(&r, "26-20105266"), vec![0]);
        assert_eq!(corrispondenze(&r, "2026-20105267"), vec![1]);
        assert_eq!(corrispondenze(&r, "25-105266"), vec![2]);
        // too short or unknown
        assert!(corrispondenze(&r, "12").is_empty());
        assert!(corrispondenze(&r, "999999").is_empty());
        // trailing newline / spaces from the reader
        assert_eq!(corrispondenze(&r, " 20105267\r\n"), vec![1]);
    }
}
