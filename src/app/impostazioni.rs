//! Settings editor for the shared `Esprit.ini` (gear button on the login screen).
//! Every line, comment and the order of the file are kept; only changed values are
//! rewritten, after a dated backup copy next to the file. The comment lines above a key
//! (the file's own notes, e.g. ";Y se monitor touch") are shown as its description.

use std::path::{Path, PathBuf};

use chrono::Local;
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::{Dialogo, Main, Scopo};
use crate::theme::*;
use crate::ui::{self as w, Icon, Kind};

pub(super) enum Riga {
    /// Comments, blank lines and anything else: written back as is.
    Testo(String),
    Sezione(String, String),
    /// `nuova`: a key the original reads but the file does not contain yet; written only
    /// once it gets a value.
    Voce { raw: String, prima: String, chiave: String, valore: String, originale: String, descr: String, nuova: bool },
}

/// IniFile.IniReadValue (Raccolta dati 0x5c6d8) reads with GetPrivateProfileString into a
/// 255-char buffer: longer values reach the original cut to 254 characters.
pub(super) const MAX_VALORE: usize = 254;

/// The keys Program::Main (0x5d984) reads, with what each one does there.
const CHIAVI_ORIGINALE: [(&str, &str, &str); 25] = [
    ("Connect", "SERVERSQL", "Server SQL"),
    ("Connect", "DATABASESQL", "Database"),
    ("Connect", "UIDSQL", "Utente SQL"),
    ("Connect", "PWDSQL", "Password SQL"),
    ("Raccolta dati", "BloccoCommesse", "Commesse per pagina (obbligatorio per l'originale)"),
    ("Raccolta dati", "MACCHINE", "Codici macchine della postazione separati da , (* = tutte)"),
    ("Raccolta dati", "OPERATORI", "Codici operatori ammessi separati da , (vuoto = tutti)"),
    ("Raccolta dati", "MaxLengthNoteCommessa", "Lunghezza massima note commessa (vuoto = 80)"),
    ("Raccolta dati", "DISPLAYRIDOTTO", "Y se il monitor ha bassa risoluzione"),
    ("Raccolta dati", "TOUCH", "Y se monitor touch"),
    ("Raccolta dati", "InsParziale", "S = versamenti parziali (controlla la quantità già versata)"),
    ("Raccolta dati", "REGISTRA_PESATE", "Y per registrare le pesate"),
    ("Raccolta dati", "MOSTRA_INPUT_NON_ASSEGNATI", "Y per mostrare la scheda degli input driver non assegnati"),
    ("Raccolta dati", "AutoRefreshCommesse", "Y per aggiornare le commesse automaticamente"),
    ("Raccolta dati", "FermiMacchinaDay", "S per mostrare solo i fermi macchina del giorno"),
    ("Raccolta dati", "MesRidotto", "Y per la visualizzazione MES ridotta"),
    ("Raccolta dati", "USERNAME_PSW_STRING", "Y per codice e password operatore con lettere"),
    ("Raccolta dati", "USA_BADGE_OPERATORE", "Y per l'accesso operatore con badge"),
    ("Raccolta dati", "TORNA_SCHEDA_MACCHINE_DOPO_OPERAZIONI", "Y per tornare alla scheda macchine dopo le operazioni"),
    ("Raccolta dati", "CONSTRSQLINPUTDRIVER", "Connessione al database degli input driver (contatori e tempi)"),
    ("Raccolta dati", "CONSTROASI", "Connessione al database OASI (giacenze)"),
    ("Raccolta dati", "SCHEMAOASI", "Schema del database OASI"),
    ("Raccolta dati", "ArticoloLabel", "Etichetta personalizzata del campo articolo"),
    ("Raccolta dati", "PRINTER_EXE_PATH", "Programma per la stampa delle etichette"),
    ("Raccolta dati", "IDWSINS", "Identificativo postazione per gli inserimenti"),
];

pub(super) struct EditorIni {
    pub path: PathBuf,
    pub righe: Vec<Riga>,
    crlf: bool,
    latin1: bool,
    bom: bool,
    fine_eol: bool,
    pub mostra_segreti: bool,
    pub esito: Option<(String, bool)>,
    pub salvato: bool,
}

/// Keys the original reads as a flag (Program::Main 0x5d984): `== "Y"` (MOSTRA_INPUT_NON_ASSEGNATI
/// `!= "Y"`), InsParziale and FermiMacchinaDay `== "S"`.
pub(super) fn si_no(chiave: &str, originale: &str) -> Option<(&'static str, &'static str)> {
    let k = chiave.to_uppercase();
    if k == "INSPARZIALE" || k == "FERMIMACCHINADAY" {
        return Some(("S", "N"));
    }
    const FLAG: [&str; 10] = [
        CHIAVE_SCHERMO,
        "TOUCH",
        "DISPLAYRIDOTTO",
        "MOSTRA_INPUT_NON_ASSEGNATI",
        "REGISTRA_PESATE",
        "USA_BADGE_OPERATORE",
        "TORNA_SCHEDA_MACCHINE_DOPO_OPERAZIONI",
        "USERNAME_PSW_STRING",
        "AUTOREFRESHCOMMESSE",
        "MESRIDOTTO",
    ];
    if FLAG.contains(&k.as_str()) || matches!(originale, "Y" | "N") {
        Some(("Y", "N"))
    } else {
        None
    }
}

/// Passwords (and connection strings holding one) are hidden unless shown on request.
pub(super) fn segreto(chiave: &str, valore: &str) -> bool {
    let k = chiave.to_uppercase();
    k.contains("PWD") || k.contains("PASSWORD") || valore.to_lowercase().contains("password=")
}

/// This app's own keys (the original ignores them: it reads only the keys above).
pub(super) const CHIAVE_SCHERMO: &str = "SCHERMO_INTERO";
const CHIAVI_APP: [(&str, &str, &str); 1] = [("Raccolta dati", CHIAVE_SCHERMO, "Y = schermo intero (solo questo programma)")];

/// Keys the original reads: description when the file has no note, and the missing ones
/// added (empty, `nuova`) after the last key of their section.
fn completa(righe: &mut Vec<Riga>) {
    for r in righe.iter_mut() {
        if let Riga::Voce { chiave, descr, .. } = r {
            if descr.is_empty() {
                if let Some((_, _, d)) = CHIAVI_ORIGINALE.iter().chain(CHIAVI_APP.iter()).find(|(_, k, _)| k.eq_ignore_ascii_case(chiave)) {
                    *descr = d.to_string();
                }
            }
        }
    }
    let sezioni: Vec<(usize, String)> = righe.iter().enumerate().filter_map(|(i, r)| if let Riga::Sezione(_, n) = r { Some((i, n.clone())) } else { None }).collect();
    for (si, nome) in sezioni.into_iter().rev() {
        let fine = righe.iter().enumerate().skip(si + 1).find(|(_, r)| matches!(r, Riga::Sezione(..))).map(|(i, _)| i).unwrap_or(righe.len());
        let presenti: Vec<String> = righe[si + 1..fine].iter().filter_map(|r| if let Riga::Voce { chiave, .. } = r { Some(chiave.to_lowercase()) } else { None }).collect();
        let dopo = righe[si + 1..fine].iter().rposition(|r| matches!(r, Riga::Voce { .. })).map(|p| si + 2 + p).unwrap_or(si + 1);
        let mancanti: Vec<Riga> = CHIAVI_ORIGINALE
            .iter()
            .chain(CHIAVI_APP.iter())
            .filter(|(s, k, _)| s.eq_ignore_ascii_case(&nome) && !presenti.contains(&k.to_lowercase()))
            .map(|(_, k, d)| Riga::Voce { raw: String::new(), prima: k.to_string(), chiave: k.to_string(), valore: String::new(), originale: String::new(), descr: d.to_string(), nuova: true })
            .collect();
        for (j, r) in mancanti.into_iter().enumerate() {
            righe.insert(dopo + j, r);
        }
    }
}

impl EditorIni {
    pub fn apri(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Impossibile leggere {}: {e}", path.display()))?;
        let (text, latin1) = match String::from_utf8(bytes.clone()) {
            Ok(s) => (s, false),
            Err(_) => (bytes.iter().map(|&b| b as char).collect(), true),
        };
        let bom = text.starts_with('\u{feff}');
        let text = text.trim_start_matches('\u{feff}');
        let crlf = text.contains("\r\n");
        let fine_eol = text.ends_with('\n');
        let mut righe = Vec::new();
        let mut note: Vec<String> = Vec::new();
        for raw in text.lines() {
            let t = raw.trim();
            if t.starts_with('[') && t.ends_with(']') {
                righe.push(Riga::Sezione(raw.to_string(), t[1..t.len() - 1].trim().to_string()));
                note.clear();
            } else if t.starts_with(';') || t.starts_with('#') {
                let c = t.trim_start_matches([';', '#']).trim();
                if !c.is_empty() {
                    note.push(c.to_string());
                }
                righe.push(Riga::Testo(raw.to_string()));
            } else if let Some((k, v)) = raw.split_once('=') {
                righe.push(Riga::Voce {
                    raw: raw.to_string(),
                    prima: k.to_string(),
                    chiave: k.trim().to_string(),
                    valore: v.trim().to_string(),
                    originale: v.trim().to_string(),
                    descr: note.join(" "),
                    nuova: false,
                });
                note.clear();
            } else {
                if t.is_empty() {
                    note.clear();
                }
                righe.push(Riga::Testo(raw.to_string()));
            }
        }
        completa(&mut righe);
        Ok(Self { path: path.to_path_buf(), righe, crlf, latin1, bom, fine_eol, mostra_segreti: false, esito: None, salvato: false })
    }

    /// Full-screen value in the editor and in the file (None if the file has no [Raccolta dati]).
    pub fn schermo(&self) -> Option<(bool, bool)> {
        self.righe.iter().find_map(|r| match r {
            Riga::Voce { chiave, valore, originale, .. } if chiave.eq_ignore_ascii_case(CHIAVE_SCHERMO) => Some((valore == "Y", originale == "Y")),
            _ => None,
        })
    }

    fn imposta_schermo(&mut self, on: bool) {
        for r in &mut self.righe {
            if let Riga::Voce { chiave, valore, .. } = r {
                if chiave.eq_ignore_ascii_case(CHIAVE_SCHERMO) {
                    *valore = if on { "Y" } else { "N" }.to_string();
                }
            }
        }
    }

    pub fn modifiche(&self) -> usize {
        self.righe.iter().filter(|r| matches!(r, Riga::Voce { valore, originale, .. } if valore != originale)).count()
    }

    pub fn annulla(&mut self) {
        for r in &mut self.righe {
            if let Riga::Voce { valore, originale, .. } = r {
                *valore = originale.clone();
            }
        }
    }

    pub fn testo(&self) -> String {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let mut righe: Vec<String> = Vec::new();
        for r in &self.righe {
            match r {
                Riga::Testo(s) | Riga::Sezione(s, _) => righe.push(s.clone()),
                // a key missing from the file stays out until it gets a value
                Riga::Voce { nuova: true, valore, .. } if valore.is_empty() => {}
                // unchanged lines are written exactly as they were
                Riga::Voce { raw, valore, originale, nuova: false, .. } if valore == originale => righe.push(raw.clone()),
                Riga::Voce { prima, valore, .. } => righe.push(format!("{prima}={valore}")),
            }
        }
        let mut out = String::new();
        if self.bom {
            out.push('\u{feff}');
        }
        out.push_str(&righe.join(eol));
        if self.fine_eol {
            out.push_str(eol);
        }
        out
    }

    /// Backup copy `Esprit.ini.bak-YYYYMMDD-HHMMSS`, then the new file (same encoding).
    pub fn salva(&mut self) -> Result<PathBuf, String> {
        let out = self.testo();
        let bytes: Vec<u8> = if self.latin1 { out.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect() } else { out.into_bytes() };
        let nome = self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Esprit.ini".into());
        let copia = self.path.with_file_name(format!("{nome}.bak-{}", Local::now().format("%Y%m%d-%H%M%S")));
        std::fs::copy(&self.path, &copia).map_err(|e| format!("Copia di sicurezza non riuscita ({}): {e}", copia.display()))?;
        let tmp = self.path.with_file_name(format!("{nome}.tmp"));
        std::fs::write(&tmp, &bytes).map_err(|e| format!("Scrittura non riuscita ({}): {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("Sostituzione di {} non riuscita: {e}", self.path.display()))?;
        for r in &mut self.righe {
            if let Riga::Voce { raw, prima, valore, originale, nuova, .. } = r {
                if valore != originale {
                    *raw = format!("{prima}={valore}");
                    *originale = valore.clone();
                    if !valore.is_empty() {
                        *nuova = false;
                    }
                }
            }
        }
        Ok(copia)
    }
}

impl Main {
    pub(super) fn apri_impostazioni(&mut self) {
        match EditorIni::apri(&self.ini_path) {
            Ok(e) => self.impostazioni = Some(e),
            Err(e) => self.messaggio(e, true),
        }
    }

    /// Full-screen editor over the login screen.
    pub(super) fn editor_impostazioni(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let sola_lettura = self.sola_lettura;
        let mut chiudi = false;
        let mut salva = false;
        let mut riavvia = false;
        let mut schermo_live: Option<bool> = None;
        let mut tastiera: Option<Dialogo> = None;
        egui::Area::new(egui::Id::new("ini-dim")).fade_in(false).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(90));
        });
        let area = screen.shrink2(vec2(40.0, 30.0));
        egui::Area::new(egui::Id::new("ini")).fade_in(false).order(egui::Order::Middle).fixed_pos(area.min).show(ctx, |ui| {
            let Some(ed) = &mut self.impostazioni else { return };
            let (r, _) = ui.allocate_exact_size(area.size(), Sense::hover());
            let p = ui.painter().clone();
            w::shadow(&p, r, 18, 40);
            p.rect_filled(r, 18.0, BG_TOP);
            p.rect_stroke(r, 18.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
            w::icon(&p, Rect::from_min_size(r.left_top() + vec2(28.0, 26.0), vec2(40.0, 40.0)), Icon::Gear, PRIMARY);
            p.text(r.left_top() + vec2(84.0, 22.0), Align2::LEFT_TOP, "Impostazioni postazione", bold(26.0), TEXT);
            p.text(r.left_top() + vec2(84.0, 58.0), Align2::LEFT_TOP, format!("{}  ·  valori letti anche dal programma originale", ed.path.display()), reg(16.0), MUTED);
            let tr = Rect::from_min_size(pos2(r.right() - 28.0 - 260.0, r.top() + 22.0), vec2(260.0, 56.0));
            let mut tui = ui.new_child(egui::UiBuilder::new().max_rect(tr));
            let lbl = if ed.mostra_segreti { "NASCONDI PASSWORD" } else { "MOSTRA PASSWORD" };
            if w::tile(&mut tui, tr.size(), Some(Icon::Key), lbl, if ed.mostra_segreti { Kind::Orange } else { Kind::Normal }, true).clicked() {
                ed.mostra_segreti = !ed.mostra_segreti;
            }

            // First option: full screen (applies at once; SALVA keeps it for the next start)
            let mut top = r.top() + 96.0;
            if let Some((on, _)) = ed.schermo() {
                let cr = Rect::from_min_size(pos2(r.left() + 28.0, top), vec2(r.width() - 56.0, 80.0));
                p.rect_filled(cr, 12.0, lerp(PRIMARY, Color32::WHITE, 0.92));
                p.rect_stroke(cr, 12.0, Stroke::new(1.0, lerp(PRIMARY, Color32::WHITE, 0.6)), StrokeKind::Inside);
                w::icon(&p, Rect::from_min_size(cr.left_center() + vec2(18.0, -18.0), vec2(36.0, 36.0)), Icon::Layers, PRIMARY_DARK);
                p.text(cr.left_top() + vec2(70.0, 14.0), Align2::LEFT_TOP, "Schermo intero", bold(21.0), TEXT);
                p.text(cr.left_top() + vec2(70.0, 44.0), Align2::LEFT_TOP, "Nasconde la barra di Windows e il bordo della finestra · F11 lo attiva al volo", reg(15.0), MUTED);
                let sr = Rect::from_min_max(pos2(cr.right() - 12.0 - 330.0, cr.top() + 10.0), pos2(cr.right() - 12.0, cr.bottom() - 10.0));
                let mut sui = ui.new_child(egui::UiBuilder::new().max_rect(sr).layout(egui::Layout::left_to_right(egui::Align::Center)));
                sui.spacing_mut().item_spacing.x = 10.0;
                if w::tile(&mut sui, vec2(160.0, sr.height()), None, "SÌ", if on { Kind::Good } else { Kind::Normal }, true).clicked() && !on {
                    ed.imposta_schermo(true);
                    schermo_live = Some(true);
                }
                if w::tile(&mut sui, vec2(160.0, sr.height()), None, "NO", if on { Kind::Normal } else { Kind::Danger }, true).clicked() && on {
                    ed.imposta_schermo(false);
                    schermo_live = Some(false);
                }
                top = cr.bottom() + 10.0;
            }

            // Rows
            let list = Rect::from_min_max(pos2(r.left() + 28.0, top), pos2(r.right() - 28.0, r.bottom() - 100.0));
            let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(list));
            let mostra = ed.mostra_segreti;
            egui::ScrollArea::vertical()
                .id_salt("ini-righe")
                .auto_shrink([false, false])
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .show(&mut lui, |ui| {
                    let wd = ui.available_width() - 14.0;
                    for (i, riga) in ed.righe.iter_mut().enumerate() {
                        match riga {
                            Riga::Sezione(_, nome) => {
                                ui.add_space(10.0);
                                let (hr, _) = ui.allocate_exact_size(vec2(wd, 36.0), Sense::hover());
                                ui.painter().text(hr.left_center(), Align2::LEFT_CENTER, format!("[{nome}]"), bold(21.0), PRIMARY_DARK);
                                ui.painter().line_segment([hr.left_bottom(), hr.right_bottom()], Stroke::new(1.0, BORDER));
                                ui.add_space(4.0);
                            }
                            // shown in the card at the top
                            Riga::Voce { chiave, .. } if chiave.eq_ignore_ascii_case(CHIAVE_SCHERMO) => {}
                            Riga::Voce { chiave, valore, originale, descr, nuova, .. } => {
                                let (rr, _) = ui.allocate_exact_size(vec2(wd, 74.0), Sense::hover());
                                let cambiato = valore != originale;
                                let p = ui.painter();
                                p.rect_filled(rr, 10.0, if cambiato { SEL_FILL } else if *nuova { BG_TOP } else { CARD });
                                p.rect_stroke(rr, 10.0, if cambiato { Stroke::new(2.0, SEL_BORDER) } else { Stroke::new(1.0, BORDER) }, StrokeKind::Inside);
                                let sx = rr.width() * 0.52;
                                p.text(rr.left_top() + vec2(16.0, 14.0), Align2::LEFT_TOP, w::elide(p, chiave, bold(19.0), sx - 24.0), bold(19.0), if *nuova && !cambiato { MUTED } else { TEXT });
                                let d = match (descr.is_empty(), *nuova) {
                                    (_, true) => format!("Non presente nel file · {descr}"),
                                    (true, false) => "—".to_string(),
                                    (false, false) => descr.clone(),
                                };
                                p.text(rr.left_top() + vec2(16.0, 42.0), Align2::LEFT_TOP, w::elide(p, &d, reg(15.0), sx - 24.0), reg(15.0), MUTED);
                                let vr = Rect::from_min_max(pos2(rr.left() + sx, rr.top() + 10.0), pos2(rr.right() - 12.0, rr.bottom() - 10.0));
                                if let Some((si, no)) = si_no(chiave, originale) {
                                    // yes / no switch
                                    let half = (vr.width() - 10.0) / 2.0;
                                    let mut vui = ui.new_child(egui::UiBuilder::new().max_rect(vr).layout(egui::Layout::left_to_right(egui::Align::Center)));
                                    vui.spacing_mut().item_spacing.x = 10.0;
                                    let on = valore.as_str() == si;
                                    if w::tile(&mut vui, vec2(half, vr.height()), None, &format!("SÌ ({si})"), if on { Kind::Good } else { Kind::Normal }, true).clicked() {
                                        *valore = si.to_string();
                                    }
                                    let off = valore.as_str() == no || (valore.is_empty() && no == "N");
                                    if w::tile(&mut vui, vec2(half, vr.height()), None, &format!("NO ({no})"), if off { Kind::Danger } else { Kind::Normal }, true).clicked() {
                                        *valore = no.to_string();
                                    }
                                } else {
                                    // value box: tap to edit with the touch keyboard
                                    let resp = ui.interact(vr, ui.id().with(("ini", i)), Sense::click());
                                    let p = ui.painter();
                                    p.rect_filled(vr, 8.0, CARD_SOFT);
                                    p.rect_stroke(vr, 8.0, Stroke::new(1.0, BORDER_HI), StrokeKind::Inside);
                                    let nascosto = segreto(chiave, valore) && !mostra;
                                    let shown = if valore.is_empty() {
                                        "(vuoto)".to_string()
                                    } else if nascosto {
                                        "●".repeat(valore.chars().count().min(12))
                                    } else {
                                        valore.clone()
                                    };
                                    p.text(vr.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, w::elide(p, &shown, reg(18.0), vr.width() - 28.0), reg(18.0), if valore.is_empty() { FAINT } else { TEXT });
                                    if resp.clicked() {
                                        tastiera = Some(Dialogo::Tastierino {
                                            titolo: chiave.clone(),
                                            valore: valore.clone(),
                                            password: nascosto,
                                            trattino: true,
                                            decimali: false,
                                            scopo: Scopo::ValoreIni(i),
                                        });
                                    }
                                }
                                ui.add_space(6.0);
                            }
                            Riga::Testo(_) => {}
                        }
                    }
                });

            // Bottom: outcome + buttons
            let bot = Rect::from_min_size(pos2(r.left() + 28.0, r.bottom() - 88.0), vec2(r.width() - 56.0, 72.0));
            let n = ed.modifiche();
            let (msg, err) = match &ed.esito {
                Some((m, e)) => (m.clone(), *e),
                None if sola_lettura => ("Sola lettura: le modifiche non possono essere salvate.".to_string(), true),
                None if n > 0 => (format!("{n} valori modificati: premi SALVA (verrà fatta una copia di sicurezza)."), false),
                None => ("Tocca un valore per modificarlo. Le modifiche valgono dal prossimo avvio.".to_string(), false),
            };
            p.text(bot.left_center(), Align2::LEFT_CENTER, w::elide(&p, &msg, bold(16.0), bot.width() - 700.0), bold(16.0), if err { DANGER } else { MUTED });
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(bot).layout(egui::Layout::right_to_left(egui::Align::Center)));
            bui.spacing_mut().item_spacing.x = 14.0;
            if w::tile(&mut bui, vec2(200.0, 70.0), Some(Icon::Check), "SALVA", Kind::Good, n > 0 && !sola_lettura).clicked() {
                salva = true;
            }
            if ed.salvato && n == 0 && w::tile(&mut bui, vec2(200.0, 70.0), Some(Icon::Refresh), "RIAVVIA", Kind::Orange, true).clicked() {
                riavvia = true;
            }
            if w::tile(&mut bui, vec2(200.0, 70.0), Some(Icon::Close), if n > 0 { "ANNULLA" } else { "CHIUDI" }, Kind::Normal, true).clicked() {
                chiudi = true;
            }
        });
        if let Some(t) = tastiera {
            self.dialoghi.push(t);
        }
        if salva {
            if let Some(ed) = &mut self.impostazioni {
                ed.esito = Some(match ed.salva() {
                    Ok(copia) => {
                        ed.salvato = true;
                        (format!("Salvato. Copia: {}. Premi RIAVVIA per usare i nuovi valori.", copia.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()), false)
                    }
                    Err(e) => (e, true),
                });
            }
        }
        if let Some(b) = schermo_live {
            self.schermo_intero(ctx, b);
        }
        if chiudi {
            // closed without saving: back to what the file says
            if let Some((_, nel_file)) = self.impostazioni.as_ref().and_then(|e| e.schermo()) {
                if nel_file != self.schermo_intero {
                    self.schermo_intero(ctx, nel_file);
                }
            }
            self.impostazioni = None;
        }
        if riavvia {
            // a new copy of the program with the same options, then this one closes
            let exe = std::env::current_exe();
            // a fresh launcher (main.rs): not marked as the app, it waits for our lock
            let nuovo = exe.and_then(|e| {
                std::process::Command::new(e)
                    .args(std::env::args().skip(1))
                    .env("RD_RIAVVIO", "1")
                    .env_remove("RD_FIGLIO")
                    .env_remove("RD_GRAFICA")
                    .env_remove("RD_TENTATIVO")
                    .spawn()
            });
            match nuovo {
                Ok(_) => self.chiudi_ora = true,
                Err(e) => self.messaggio(format!("Riavvio non riuscito: {e}. Chiudi e riapri il programma."), true),
            }
        }
    }

    pub(super) fn valore_ini(&mut self, i: usize, v: String) {
        if let Some(ed) = &mut self.impostazioni {
            if let Some(Riga::Voce { valore, chiave, .. }) = ed.righe.get_mut(i) {
                let v = v.trim();
                if v.chars().count() > MAX_VALORE {
                    *valore = v.chars().take(MAX_VALORE).collect();
                    ed.esito = Some((format!("{chiave}: tagliato a {MAX_VALORE} caratteri (il programma originale non legge oltre)."), true));
                } else {
                    *valore = v.to_string();
                    ed.esito = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_file_and_changes_only_values() {
        let dir = std::env::temp_dir().join(format!("rd_ini_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Esprit.ini");
        let src = "[Connect]\r\n;Parametri\r\nSERVERSQL=SRV\r\nPWDSQL = segreta\r\n\r\n[Raccolta dati]\r\n;Y se monitor touch\r\nTOUCH=Y\r\n";
        std::fs::write(&path, src).unwrap();
        let mut ed = EditorIni::apri(&path).unwrap();
        assert_eq!(ed.testo(), src, "unchanged file is written back byte for byte");
        if let Some(Riga::Voce { descr, .. }) = ed.righe.iter().find(|r| matches!(r, Riga::Voce { chiave, .. } if chiave == "TOUCH")) {
            assert_eq!(descr, "Y se monitor touch");
        }
        // keys the original reads but the file lacks are offered, empty
        assert!(ed.righe.iter().any(|r| matches!(r, Riga::Voce { chiave, nuova: true, .. } if chiave == "USERNAME_PSW_STRING")));
        for r in &mut ed.righe {
            if let Riga::Voce { chiave, valore, .. } = r {
                if chiave == "TOUCH" {
                    *valore = "N".into();
                }
                if chiave == "USERNAME_PSW_STRING" {
                    *valore = "Y".into();
                }
            }
        }
        assert_eq!(ed.modifiche(), 2);
        let copia = ed.salva().unwrap();
        let nuovo = std::fs::read_to_string(&path).unwrap();
        assert!(nuovo.contains("TOUCH=N\r\n") && nuovo.contains("PWDSQL = segreta\r\n"));
        // the new key goes right after the last key of its section
        assert!(nuovo.contains("TOUCH=N\r\nUSERNAME_PSW_STRING=Y\r\n"), "{nuovo}");
        assert!(!nuovo.contains("DATABASESQL"), "unset missing keys stay out");
        assert_eq!(std::fs::read_to_string(&copia).unwrap(), src);
        assert_eq!(ed.modifiche(), 0);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(si_no("InsParziale", "S"), Some(("S", "N")));
        assert!(segreto("PWDSQL", "x") && segreto("CONSTRSQLINPUTDRIVER", "Data Source=a;Password=b;"));
    }
}
