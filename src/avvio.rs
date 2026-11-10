//! Start-up splash: every step of the start (settings file, SQL server, login,
//! MES settings, machines, save helper, first data) with its state and time, so
//! a start that hangs or fails says where and why. Each change is also appended
//! to %LOCALAPPDATA%\RaccoltaDati\avvio.log.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::theme::*;
use crate::ui::{self as w, Icon, Kind};

pub const INI: usize = 0;
pub const SERVER: usize = 1;
pub const DATABASE: usize = 2;
pub const SETUP: usize = 3;
pub const MACCHINE: usize = 4;
pub const PONTE: usize = 5;
pub const DATI: usize = 6;

const NOMI: [&str; 7] = [
    "Lettura impostazioni (Esprit.ini)",
    "Raggiungimento server SQL",
    "Accesso al database e aziende",
    "Impostazioni MES",
    "Macchine della postazione",
    "Modulo di salvataggio",
    "Caricamento commesse",
];

#[derive(Clone, Debug)]
pub enum Stato {
    Attesa,
    InCorso(Instant),
    Fatto(u128),
    Avviso(String),
    Errore(String),
}

#[derive(Clone, Debug)]
pub struct Passo {
    pub nome: &'static str,
    pub dettaglio: String,
    pub stato: Stato,
}

#[derive(Clone)]
pub struct Passi {
    inner: Arc<Mutex<Vec<Passo>>>,
    pub avviato: Instant,
}

pub fn log_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    base.join("RaccoltaDati").join("avvio.log")
}

pub fn log(line: &str) {
    let p = log_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{}  {line}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
    }
}

impl Passi {
    pub fn new() -> Self {
        let v = NOMI.iter().map(|n| Passo { nome: n, dettaglio: String::new(), stato: Stato::Attesa }).collect();
        Self { inner: Arc::new(Mutex::new(v)), avviato: Instant::now() }
    }

    fn set(&self, i: usize, stato: Stato, dettaglio: Option<String>) {
        let mut v = self.inner.lock().unwrap();
        if let Some(p) = v.get_mut(i) {
            if let Some(d) = dettaglio {
                p.dettaglio = d;
            }
            let txt = match &stato {
                Stato::Attesa => return,
                Stato::InCorso(_) => "in corso".to_string(),
                Stato::Fatto(ms) => format!("ok ({ms} ms)"),
                Stato::Avviso(e) => format!("AVVISO: {e}"),
                Stato::Errore(e) => format!("ERRORE: {e}"),
            };
            log(&format!("{} — {txt}{}", p.nome, if p.dettaglio.is_empty() { String::new() } else { format!(" [{}]", p.dettaglio) }));
            p.stato = stato;
        }
    }

    pub fn inizia(&self, i: usize, dettaglio: impl Into<String>) {
        self.set(i, Stato::InCorso(Instant::now()), Some(dettaglio.into()));
    }

    pub fn fatto(&self, i: usize) {
        let ms = match self.inner.lock().unwrap().get(i).map(|p| p.stato.clone()) {
            Some(Stato::InCorso(t)) => t.elapsed().as_millis(),
            _ => 0,
        };
        self.set(i, Stato::Fatto(ms), None);
    }

    pub fn fatto_con(&self, i: usize, dettaglio: impl Into<String>) {
        let ms = match self.inner.lock().unwrap().get(i).map(|p| p.stato.clone()) {
            Some(Stato::InCorso(t)) => t.elapsed().as_millis(),
            _ => 0,
        };
        self.set(i, Stato::Fatto(ms), Some(dettaglio.into()));
    }

    pub fn avviso(&self, i: usize, msg: impl Into<String>) {
        self.set(i, Stato::Avviso(msg.into()), None);
    }

    pub fn errore(&self, i: usize, msg: impl Into<String>) {
        self.set(i, Stato::Errore(msg.into()), None);
    }

    /// Marks the step in progress (if any) as failed with `msg`.
    pub fn errore_corrente(&self, msg: &str) {
        let i = self.inner.lock().unwrap().iter().position(|p| matches!(p.stato, Stato::InCorso(_)));
        if let Some(i) = i {
            self.errore(i, msg);
        }
    }

    pub fn elenco(&self) -> Vec<Passo> {
        self.inner.lock().unwrap().clone()
    }

    pub fn ha_errori(&self) -> bool {
        self.inner.lock().unwrap().iter().any(|p| matches!(p.stato, Stato::Errore(_)))
    }
}

pub enum Scelta {
    Niente,
    Riprova,
    Continua,
    Azienda(String, String),
}

/// The splash card. `aziende`: shown as buttons when there is more than one.
pub fn splash(ui: &mut Ui, passi: &Passi, aziende: &[(String, String)], continua: bool) -> Scelta {
    let mut scelta = Scelta::Niente;
    let screen = ui.max_rect();
    let p = ui.painter().clone();
    p.rect_filled(screen, 0.0, BG);
    let elenco = passi.elenco();
    let errori = elenco.iter().any(|x| matches!(x.stato, Stato::Errore(_)));
    let extra = if aziende.len() > 1 || errori || continua { 110.0 } else { 40.0 };
    let card = Rect::from_center_size(screen.center(), vec2(760.0, 170.0 + elenco.len() as f32 * 54.0 + extra));
    w::shadow(&p, card, 20, 34);
    p.rect_filled(card, 20.0, CARD);
    w::logo_aziendale(&p, Rect::from_min_size(card.left_top() + vec2(32.0, 26.0), vec2(330.0, 60.0)));
    let secs = passi.avviato.elapsed().as_secs_f32();
    let sotto = if errori { "Avvio interrotto: vedi il passo in rosso".to_string() } else { format!("Avvio in corso…  {secs:.1} s") };
    p.text(card.right_top() + vec2(-32.0, 56.0), Align2::RIGHT_CENTER, sotto, reg(17.0), if errori { DANGER } else { MUTED });

    let mut y = card.top() + 130.0;
    for passo in &elenco {
        let row = Rect::from_min_size(pos2(card.left() + 32.0, y), vec2(card.width() - 64.0, 48.0));
        let (col, txt) = match &passo.stato {
            Stato::Attesa => (FAINT, String::new()),
            Stato::InCorso(t) => (PRIMARY, format!("{:.1} s", t.elapsed().as_secs_f32())),
            Stato::Fatto(ms) => (GOOD, format!("{ms} ms")),
            Stato::Avviso(e) => (WARN, e.clone()),
            Stato::Errore(e) => (DANGER, e.clone()),
        };
        let c = pos2(row.left() + 16.0, row.center().y);
        match &passo.stato {
            Stato::InCorso(_) => {
                ui.put(Rect::from_center_size(c, vec2(24.0, 24.0)), egui::Spinner::new().size(22.0).color(PRIMARY));
            }
            Stato::Attesa => {
                p.circle_stroke(c, 10.0, Stroke::new(2.0, FAINT));
            }
            _ => {
                p.circle_filled(c, 12.0, col);
                let ic = match passo.stato {
                    Stato::Fatto(_) => Icon::Check,
                    _ => Icon::Close,
                };
                w::icon(&p, Rect::from_center_size(c, vec2(14.0, 14.0)), ic, Color32::WHITE);
            }
        }
        let nome_col = if matches!(passo.stato, Stato::Attesa) { FAINT } else { TEXT };
        p.text(pos2(row.left() + 42.0, row.top() + 14.0), Align2::LEFT_CENTER, passo.nome, bold(18.0), nome_col);
        if !passo.dettaglio.is_empty() {
            p.text(pos2(row.left() + 42.0, row.top() + 36.0), Align2::LEFT_CENTER, w::elide(&p, &passo.dettaglio, reg(14.0), 330.0), reg(14.0), MUTED);
        }
        if !txt.is_empty() {
            let max = row.width() - 400.0;
            let t = w::elide(&p, &txt, reg(15.0), max);
            p.text(pos2(row.right(), row.center().y), Align2::RIGHT_CENTER, t, if matches!(passo.stato, Stato::Errore(_) | Stato::Avviso(_)) { bold(15.0) } else { reg(15.0) }, col);
        }
        y += 54.0;
    }

    let bottom = Rect::from_min_max(pos2(card.left() + 32.0, y + 10.0), pos2(card.right() - 32.0, card.bottom() - 20.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bottom).layout(egui::Layout::left_to_right(egui::Align::Center)));
    b.spacing_mut().item_spacing.x = 12.0;
    if errori {
        if w::tile(&mut b, vec2(200.0, 62.0), Some(Icon::Refresh), "RIPROVA", Kind::Primary, true).clicked() {
            scelta = Scelta::Riprova;
        }
        if continua && w::tile(&mut b, vec2(240.0, 62.0), Some(Icon::Play), "CONTINUA", Kind::Normal, true).clicked() {
            scelta = Scelta::Continua;
        }
        let lp = log_path();
        b.label(egui::RichText::new(format!("Registro: {}", lp.display())).size(13.0).color(MUTED));
    } else if aziende.len() > 1 {
        b.label(egui::RichText::new("Azienda:").size(18.0).color(MUTED));
        for (id, nome) in aziende {
            if w::tile(&mut b, vec2(220.0, 62.0), None, &format!("{id} · {nome}"), Kind::Normal, true).clicked() {
                scelta = Scelta::Azienda(id.clone(), nome.clone());
            }
        }
    } else if continua && w::tile(&mut b, vec2(240.0, 62.0), Some(Icon::Play), "CONTINUA", Kind::Normal, true).clicked() {
        scelta = Scelta::Continua;
    }
    p.rect_stroke(card, 20.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    ui.allocate_rect(screen, Sense::hover());
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    scelta
}
