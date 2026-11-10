//! Dialogs (message, yes/no, close-phase window, number pad, exit), the
//! quantity editor (ESP_FormPezziRag) and the busy overlay.

use std::time::Duration;

use eframe::egui::{self, Align2, Color32, Margin, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{Dialogo, Main, RigaVers, Scopo, corrispondenze};
use crate::dati;
use crate::theme::*;
use crate::ui::{self as w, Icon, Kind};

/// Quantity pre-filled after a scan: the order's quantity to do, or what is
/// still missing when VERSA_PEZZI_RIMANENTI is on (same as bVersaTutto_Click 0x4f3cc).
pub(super) fn proposta(r: &RigaVers, rimanenti: bool) -> f64 {
    if rimanenti { r.rimanenti } else { r.da_fare }
}

impl Main {
    /// Match of a scanned code: one order → confirmation card; several → list; none → message.
    pub(super) fn codice_letto(&mut self, codice: String) {
        let Some(v) = &mut self.vers else { return };
        v.ultimo_codice = codice.clone();
        let trovate = corrispondenze(&v.righe, &codice);
        match trovate.len() {
            0 => {
                // rows only list what is still to record (InsParziale): a missing order may be complete
                let testo = if v.righe.is_empty() {
                    format!("Codice «{codice}»: niente da versare in questa fase, le quantità risultano già tutte registrate.")
                } else {
                    format!("Codice «{codice}»: nessuna commessa ancora da versare in questa fase corrisponde (potrebbe essere già versata completamente).")
                };
                if v.rapido {
                    self.ripristina_rapido(Some(testo));
                } else {
                    self.messaggio(testo, true);
                }
            }
            1 => {
                let idx = trovate[0];
                let qta = proposta(&v.righe[idx], v.versa_rimanenti);
                v.evidenzia = Some(idx);
                v.scorri = true;
                self.dialoghi.push(Dialogo::ScanConferma { codice, idx, qta });
            }
            _ => self.dialoghi.push(Dialogo::ScanScelta { codice, indici: trovate }),
        }
    }

    pub(super) fn editor_versamento(&mut self, ctx: &egui::Context) {
        let mut chiudi = false;
        let mut conferma = false;
        let mut barcode = false;
        let mut tastierino: Option<Dialogo> = None;
        let screen = ctx.content_rect();
        let ndec_default = self.setup.ndec;
        egui::Area::new(egui::Id::new("vers-dim")).fade_in(false).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(90));
        });
        let area = screen.shrink2(vec2(40.0, 30.0));
        egui::Area::new(egui::Id::new("vers")).fade_in(false).order(egui::Order::Middle).fixed_pos(area.min).show(ctx, |ui| {
            let Some(v) = &mut self.vers else { return };
            let (r, _) = ui.allocate_exact_size(area.size(), Sense::hover());
            let p = ui.painter().clone();
            w::shadow(&p, r, 18, 40);
            p.rect_filled(r, 18.0, BG_TOP);
            p.rect_stroke(r, 18.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
            w::icon(&p, Rect::from_min_size(r.left_top() + vec2(28.0, 26.0), vec2(40.0, 40.0)), Icon::Cube, PRIMARY);
            let titolo = if v.ultima { "Versamento prodotti finiti" } else { "Avanzamento fase" };
            p.text(r.left_top() + vec2(84.0, 22.0), Align2::LEFT_TOP, titolo, bold(26.0), TEXT);
            let extra = if v.solo_buoni.is_empty() { String::new() } else { format!("   ·   {}", v.solo_buoni) };
            p.text(r.left_top() + vec2(84.0, 58.0), Align2::LEFT_TOP, format!("Fase: {}   ·   Operazione: {}{}", v.fase, v.operazione, extra), reg(17.0), MUTED);
            let totale: f64 = v.righe.iter().filter_map(|x| x.qta).sum();
            p.text(r.right_top() + vec2(-28.0, 30.0), Align2::RIGHT_TOP, format!("Totale da versare: {}", dati::fmt_qta(totale, ndec_default)), bold(22.0), PRIMARY_DARK);

            // Tools
            let tools = Rect::from_min_size(r.left_top() + vec2(28.0, 96.0), vec2(r.width() - 56.0, 64.0));
            let mut tui = ui.new_child(egui::UiBuilder::new().max_rect(tools).layout(egui::Layout::left_to_right(egui::Align::Center)));
            tui.spacing_mut().item_spacing.x = 12.0;
            if w::tile(&mut tui, vec2(210.0, 60.0), Some(Icon::Check), "VERSA TUTTO", Kind::Normal, true).clicked() {
                // bVersaTutto_Click: the quantity to do, or what is still missing (VERSA_PEZZI_RIMANENTI)
                let filtro = v.filtro.clone();
                for x in v.righe.iter_mut().filter(|x| filtro.is_empty() || x.num.contains(&filtro)) {
                    x.qta = Some(if v.versa_rimanenti { x.rimanenti } else { x.da_fare });
                }
            }
            if w::tile(&mut tui, vec2(160.0, 60.0), Some(Icon::Close), "AZZERA", Kind::Normal, true).clicked() {
                for x in v.righe.iter_mut() {
                    x.qta = None;
                }
            }
            // BARCODE (right side of the tool row): the reader listens only in its window
            {
                let br = Rect::from_min_max(pos2(tools.right() - 250.0, tools.top() + 2.0), pos2(tools.right(), tools.bottom() - 2.0));
                let mut bui = tui.new_child(egui::UiBuilder::new().max_rect(br));
                if w::tile(&mut bui, br.size(), Some(Icon::Barcode), "BARCODE", Kind::Orange, !v.rapido).clicked() {
                    barcode = true;
                }
                // room left after VERSA TUTTO, AZZERA and FILTRA NUMERO
                let spazio = (br.left() - 16.0 - (tools.left() + 680.0)).min(240.0);
                if !v.ultimo_codice.is_empty() && spazio > 80.0 {
                    let t = format!("ultimo codice: {}", v.ultimo_codice);
                    p.text(pos2(br.left() - 16.0, br.center().y), Align2::RIGHT_CENTER, w::elide(&p, &t, reg(15.0), spazio), reg(15.0), MUTED);
                }
            }
            if v.su_singole {
                let label = if v.filtro.is_empty() { "FILTRA NUMERO".to_string() } else { format!("NUMERO: {}", v.filtro) };
                if w::tile(&mut tui, vec2(260.0, 60.0), Some(Icon::Filter), &label, if v.filtro.is_empty() { Kind::Normal } else { Kind::Good }, true).clicked() {
                    if v.filtro.is_empty() {
                        tastierino = Some(Dialogo::Tastierino {
                            titolo: "Numero commessa".into(),
                            valore: String::new(),
                            password: false,
                            trattino: false,
                            decimali: false,
                            scopo: Scopo::FiltroVersamento,
                        });
                    } else {
                        v.filtro.clear();
                    }
                }
            }

            // Rows
            let list = Rect::from_min_max(pos2(r.left() + 28.0, tools.bottom() + 12.0), pos2(r.right() - 28.0, r.bottom() - 100.0));
            let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(list));
            let col = [0.16, 0.22, 0.28, 0.12, 0.22];
            let wd = list.width();
            let qta_head = if v.label.is_empty() { "Quantità".to_string() } else { v.label.clone() };
            let head = ["Commessa", "Articolo", "Descrizione", "Da fare", qta_head.as_str()];
            let (hr, _) = lui.allocate_exact_size(vec2(wd, 34.0), Sense::hover());
            let mut x = hr.left() + 12.0;
            for (i, hd) in head.iter().enumerate() {
                lui.painter().text(pos2(x, hr.center().y), Align2::LEFT_CENTER, *hd, bold(15.0), MUTED);
                x += col[i] * wd;
            }
            let filtro = v.filtro.clone();
            let coeff = v.coeff;
            let evidenzia = v.evidenzia;
            let scorri = std::mem::take(&mut v.scorri);
            if v.righe.is_empty() {
                lui.add_space(20.0);
                lui.label(egui::RichText::new("Niente da versare per questa fase.").size(19.0).color(MUTED));
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource::ALL).show(&mut lui, |ui| {
                for (idx, riga) in v.righe.iter_mut().enumerate() {
                    if !filtro.is_empty() && !riga.num.contains(&filtro) {
                        continue;
                    }
                    let (rr, rresp) = ui.allocate_exact_size(vec2(wd - 16.0, 70.0), Sense::hover());
                    let scansionata = evidenzia == Some(idx);
                    if scansionata && scorri {
                        rresp.scroll_to_me(Some(egui::Align::Center));
                    }
                    let p = ui.painter();
                    p.rect_filled(rr, 10.0, if scansionata { lerp(GOOD, Color32::WHITE, 0.86) } else { CARD });
                    p.rect_stroke(rr, 10.0, if scansionata { Stroke::new(2.5, GOOD) } else { Stroke::new(1.0, BORDER) }, StrokeKind::Inside);
                    let mut x = rr.left() + 12.0;
                    let vals = [format!("{}-{}", riga.anno, riga.num), riga.codart.clone(), riga.descr.clone(), format!("{} {}", dati::fmt_qta(riga.da_fare, riga.ndec), riga.um)];
                    for (i, t) in vals.iter().enumerate() {
                        let font = if i == 0 { bold(19.0) } else { reg(18.0) };
                        p.text(pos2(x, rr.center().y), Align2::LEFT_CENTER, w::elide(p, t, font.clone(), col[i] * wd - 16.0), font, TEXT);
                        x += col[i] * wd;
                    }
                    let qrect = Rect::from_min_size(pos2(x, rr.top() + 9.0), vec2(col[4] * wd - 30.0, 52.0));
                    let resp = ui.interact(qrect, ui.id().with(("q", idx)), Sense::click());
                    let has = riga.qta.map(|q| q > 0.0).unwrap_or(false);
                    let p = ui.painter();
                    p.rect_filled(qrect, 8.0, if has { SEL_FILL } else { CARD_SOFT });
                    p.rect_stroke(qrect, 8.0, Stroke::new(if has { 2.0 } else { 1.0 }, if has { SEL_BORDER } else { BORDER_HI }), StrokeKind::Inside);
                    let shown = match (riga.qta, coeff) {
                        (Some(q), Some(c)) => format!("{}  ({} pz)", dati::fmt_qta(q * c, 2), dati::fmt_qta(q, riga.ndec)),
                        (Some(q), None) => dati::fmt_qta(q, riga.ndec),
                        (None, _) => "tocca per inserire".into(),
                    };
                    p.text(qrect.center(), Align2::CENTER_CENTER, shown, if has { bold(21.0) } else { reg(15.0) }, if has { PRIMARY_DARK } else { FAINT });
                    if resp.clicked() {
                        let start = match (riga.qta, coeff) {
                            (Some(q), Some(c)) => dati::fmt_qta(q * c, 2),
                            (Some(q), None) => dati::fmt_qta(q, riga.ndec),
                            _ => String::new(),
                        };
                        let start = if start.contains(',') { start.trim_end_matches('0').trim_end_matches(',').to_string() } else { start };
                        tastierino = Some(Dialogo::Tastierino {
                            titolo: format!("Quantità · {}-{}", riga.anno, riga.num),
                            valore: start,
                            password: false,
                            trattino: false,
                            decimali: riga.ndec > 0 || coeff.is_some(),
                            scopo: Scopo::QtaRiga(idx),
                        });
                    }
                    ui.add_space(4.0);
                }
            });

            // Confirm / cancel
            let bot = Rect::from_min_size(pos2(r.left() + 28.0, r.bottom() - 88.0), vec2(r.width() - 56.0, 72.0));
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(bot).layout(egui::Layout::right_to_left(egui::Align::Center)));
            bui.spacing_mut().item_spacing.x = 14.0;
            if w::tile(&mut bui, vec2(240.0, 70.0), Some(Icon::Check), "CONFERMA", Kind::Good, true).clicked() {
                conferma = true;
            }
            if w::tile(&mut bui, vec2(200.0, 70.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                chiudi = true;
            }
        });
        if let Some(t) = tastierino {
            self.dialoghi.push(t);
        }
        if barcode {
            self.apri_barcode();
        }
        if chiudi {
            if self.vers.as_ref().map(|v| v.rapido).unwrap_or(false) {
                self.ripristina_rapido(None);
            } else {
                self.vers = None;
            }
        } else if conferma {
            self.conferma_versamento();
        }
    }

    pub(super) fn dialoghi_ui(&mut self, ctx: &egui::Context) {
        let righe: Vec<RigaVers> = self.vers.as_ref().map(|v| v.righe.clone()).unwrap_or_default();
        let (rimanenti, coeff) = self.vers.as_ref().map(|v| (v.versa_rimanenti, v.coeff)).unwrap_or((false, None));
        let lettura = self.scan_buf.clone();
        let ndec = self.setup.ndec;
        let in_versamento = self.vers.is_some();
        let mac_nome = self.macchina().map(|m| if m.descr.trim().is_empty() { m.cod } else { m.descr });
        // session operators, by name when known
        let in_sessione: Vec<String> = self
            .sessione
            .iter()
            .map(|c| {
                let nome = self.turni.value.as_ref().and_then(|v| v.iter().find(|t| t.cod.trim() == c.trim())).map(|t| t.nome.trim().to_string()).unwrap_or_default();
                if nome.is_empty() { c.clone() } else { format!("{nome} ({c})") }
            })
            .collect();
        let Some(d) = self.dialoghi.last_mut() else { return };
        enum Esito {
            Niente,
            Chiudi,
            Risposta(String),
            Chiusura(f64, f64, f64),
            Valore(String),
            EsciApp,
            ScanScelto(usize),
            ScanTastierino(usize, f64),
            ScanOk(usize, f64),
            RapidoScelto(usize),
            BarcodeMano,
            EsciComunque,
            InfoAltro,
            LottoApri,
            Lotto(super::lotto::AzioneLotto),
            LottoScelto(usize),
        }
        let mut esito = Esito::Niente;
        let modal = egui::Modal::new(egui::Id::new("dialogo"))
            .area(egui::Modal::default_area(egui::Id::new("dialogo")).fade_in(false))
            .backdrop_color(Color32::from_black_alpha(110))
            .frame(egui::Frame::new().fill(CARD).corner_radius(18).inner_margin(Margin::same(28)).stroke(Stroke::new(1.0, BORDER)));
        modal.show(ctx, |ui| match d {
            Dialogo::Messaggio { testo, errore } => {
                ui.set_width(580.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                    let (bg, fg, ic) = if *errore { (lerp(DANGER, Color32::WHITE, 0.85), DANGER, Icon::Close) } else { (lerp(GOOD, Color32::WHITE, 0.85), GOOD, Icon::Check) };
                    ui.painter().circle_filled(r.center(), 23.0, bg);
                    w::icon(ui.painter(), r.shrink(12.0), ic, fg);
                    ui.add(egui::Label::new(egui::RichText::new(testo.as_str()).size(20.0).color(TEXT)).wrap());
                });
                ui.add_space(18.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::tile(ui, vec2(180.0, 64.0), None, "OK", Kind::Primary, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
            Dialogo::Domanda { testo, tre, .. } => {
                ui.set_width(660.0);
                ui.add(egui::Label::new(egui::RichText::new(testo.as_str()).size(21.0).color(TEXT)).wrap());
                ui.add_space(22.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if *tre && w::tile(ui, vec2(170.0, 66.0), None, "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Risposta("A".into());
                    }
                    if w::tile(ui, vec2(170.0, 66.0), Some(Icon::Close), "NO", Kind::Normal, true).clicked() {
                        esito = Esito::Risposta("N".into());
                    }
                    if w::tile(ui, vec2(170.0, 66.0), Some(Icon::Check), "SÌ", Kind::Primary, true).clicked() {
                        esito = Esito::Risposta("S".into());
                    }
                });
            }
            Dialogo::Chiusura { titolo, ore, minuti, operatori, .. } => {
                // ESP_FormChiudiSospFase: hours, minutes (0-59), operators (0-9, step 0.5)
                ui.set_width(640.0);
                ui.add(egui::Label::new(egui::RichText::new(titolo.as_str()).font(bold(23.0)).color(TEXT)).wrap());
                ui.add_space(16.0);
                stepper(ui, "Ore", ore, 1.0, 0.0, 9_999_999_999.0, 0);
                stepper(ui, "Minuti", minuti, 1.0, 0.0, 59.0, 0);
                stepper(ui, "Numero operatori", operatori, 0.5, 0.0, 9.0, 1);
                ui.add_space(18.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if w::tile(ui, vec2(200.0, 66.0), Some(Icon::Check), "CONFERMA", Kind::Good, true).clicked() {
                        esito = Esito::Chiusura(*ore, *minuti, *operatori);
                    }
                    if w::tile(ui, vec2(180.0, 66.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
            Dialogo::Tastierino { titolo, valore, password, trattino, decimali, scopo } => {
                // Operator code and password take letters too (the original's FormInputStringa,
                // RaccoltaDatiHelper.SelezioneOperatore with USERNAME_PSW_STRING=Y)
                let lettere = matches!(scopo, Scopo::CodiceOperatore | Scopo::Password { .. } | Scopo::ValoreIni(_));
                // settings values need paths and connection strings: extra symbols row
                let simboli = matches!(scopo, Scopo::ValoreIni(_));
                let esito_t = if lettere { tastiera(ui, titolo, valore, *password, simboli) } else { tastierino(ui, titolo, valore, *password, *trattino, *decimali) };
                if let Some(v) = esito_t {
                    esito = match v {
                        Some(v) => Esito::Valore(v),
                        None => Esito::Chiudi,
                    };
                }
            }
            Dialogo::ScanScelta { codice, indici } => {
                ui.set_width(760.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    w::icon(ui.painter(), r, Icon::Filter, PRIMARY);
                    ui.label(egui::RichText::new(format!("Codice «{codice}»: {} commesse trovate", indici.len())).font(bold(23.0)).color(TEXT));
                });
                ui.label(egui::RichText::new("Tocca la commessa da versare").size(17.0).color(MUTED));
                ui.add_space(10.0);
                egui::ScrollArea::vertical().max_height(420.0).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
                    for &i in indici.iter() {
                        let Some(r) = righe.get(i) else { continue };
                        let (rr, resp) = ui.allocate_exact_size(vec2(740.0, 76.0), Sense::click());
                        let p = ui.painter();
                        let down = resp.is_pointer_button_down_on();
                        p.rect_filled(rr, 12.0, if down { SEL_FILL } else { CARD_SOFT });
                        p.rect_stroke(rr, 12.0, Stroke::new(1.0, BORDER_HI), StrokeKind::Inside);
                        p.text(rr.left_center() + vec2(18.0, -13.0), Align2::LEFT_CENTER, format!("{}-{}", r.anno, r.num), bold(22.0), TEXT);
                        p.text(rr.left_center() + vec2(18.0, 15.0), Align2::LEFT_CENTER, w::elide(p, &format!("{} · {}", r.codart, r.descr), reg(16.0), 470.0), reg(16.0), MUTED);
                        p.text(rr.right_center() - vec2(56.0, 0.0), Align2::RIGHT_CENTER, format!("da fare {} {}", dati::fmt_qta(r.da_fare, r.ndec), r.um), bold(19.0), PRIMARY_DARK);
                        let cx = rr.right() - 26.0;
                        p.line_segment([pos2(cx - 5.0, rr.center().y - 10.0), pos2(cx + 5.0, rr.center().y)], Stroke::new(3.0, TEXT));
                        p.line_segment([pos2(cx + 5.0, rr.center().y), pos2(cx - 5.0, rr.center().y + 10.0)], Stroke::new(3.0, TEXT));
                        if resp.clicked() {
                            esito = Esito::ScanScelto(i);
                        }
                        ui.add_space(6.0);
                    }
                });
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::tile(ui, vec2(180.0, 64.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
            Dialogo::ScanConferma { codice, idx, qta } => {
                ui.set_width(700.0);
                if let Some(r) = righe.get(*idx) {
                    ui.label(egui::RichText::new(format!("Codice letto: {codice}")).size(16.0).color(MUTED));
                    ui.add_space(4.0);
                    let (card, _) = ui.allocate_exact_size(vec2(700.0, 150.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(card, 14.0, lerp(GOOD, Color32::WHITE, 0.88));
                    p.rect_stroke(card, 14.0, Stroke::new(2.0, GOOD), StrokeKind::Inside);
                    p.text(card.left_top() + vec2(22.0, 18.0), Align2::LEFT_TOP, "COMMESSA", reg(14.0), MUTED);
                    p.text(card.left_top() + vec2(22.0, 36.0), Align2::LEFT_TOP, format!("{}-{}", r.anno, r.num), bold(32.0), TEXT);
                    p.text(card.left_top() + vec2(22.0, 84.0), Align2::LEFT_TOP, w::elide(p, &r.codart, bold(18.0), 420.0), bold(18.0), TEXT);
                    p.text(card.left_top() + vec2(22.0, 112.0), Align2::LEFT_TOP, w::elide(p, &r.descr, reg(16.0), 420.0), reg(16.0), MUTED);
                    w::label_value(p, card.right_top() + vec2(-220.0, 22.0), "DA FARE", &format!("{} {}", dati::fmt_qta(r.da_fare, r.ndec), r.um), 26.0, PRIMARY_DARK, 200.0);
                    if rimanenti {
                        w::label_value(p, card.right_top() + vec2(-220.0, 84.0), "MANCANTI", &dati::fmt_qta(r.rimanenti, r.ndec), 22.0, TEXT, 200.0);
                    }
                    ui.add_space(16.0);
                    // Proposed quantity: what is to do (or still missing); − / + or tap to type
                    let label = if coeff.is_some() { "Quantità (pezzi)" } else { "Quantità da versare" };
                    ui.horizontal(|ui| {
                        let (lr, _) = ui.allocate_exact_size(vec2(230.0, 70.0), Sense::hover());
                        ui.painter().text(lr.left_center(), Align2::LEFT_CENTER, label, reg(20.0), TEXT);
                        if w::tile(ui, vec2(80.0, 66.0), None, "−", Kind::Normal, *qta > 0.0).clicked() {
                            *qta = (*qta - 1.0).max(0.0);
                        }
                        let (vr, vresp) = ui.allocate_exact_size(vec2(200.0, 66.0), Sense::click());
                        ui.painter().rect_filled(vr, 12.0, SEL_FILL);
                        ui.painter().rect_stroke(vr, 12.0, Stroke::new(2.0, SEL_BORDER), StrokeKind::Inside);
                        ui.painter().text(vr.center(), Align2::CENTER_CENTER, dati::fmt_qta(*qta, r.ndec), bold(30.0), PRIMARY_DARK);
                        if vresp.clicked() {
                            esito = Esito::ScanTastierino(*idx, *qta);
                        }
                        if w::tile(ui, vec2(80.0, 66.0), None, "+", Kind::Normal, true).clicked() {
                            *qta += 1.0;
                        }
                    });
                    ui.add_space(18.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        if w::tile(ui, vec2(220.0, 70.0), Some(Icon::Check), "CONFERMA", Kind::Good, *qta > 0.0).clicked() {
                            esito = Esito::ScanOk(*idx, *qta);
                        }
                        if w::tile(ui, vec2(180.0, 70.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                            esito = Esito::Chiudi;
                        }
                    });
                } else {
                    esito = Esito::Chiudi;
                }
            }
            Dialogo::RapidoScelta { codice, trovate } => {
                ui.set_width(800.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    w::icon(ui.painter(), r, Icon::Filter, PRIMARY);
                    let m = trovate.first().and_then(|t| t.macchine.first()).cloned().unwrap_or_default();
                    ui.label(egui::RichText::new(format!("Codice «{codice}»: {} commesse aperte su {m}", trovate.len())).font(bold(23.0)).color(TEXT));
                });
                ui.label(egui::RichText::new("Tocca la commessa da versare: verrà aperta, versata e poi la macchina torna com'era").size(17.0).color(MUTED));
                ui.add_space(10.0);
                egui::ScrollArea::vertical().max_height(430.0).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
                    for (i, t) in trovate.iter().enumerate() {
                        let (rr, resp) = ui.allocate_exact_size(vec2(780.0, 80.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(rr, 12.0, if resp.is_pointer_button_down_on() { SEL_FILL } else { CARD_SOFT });
                        p.rect_stroke(rr, 12.0, Stroke::new(1.0, BORDER_HI), StrokeKind::Inside);
                        p.text(rr.left_center() + vec2(18.0, -14.0), Align2::LEFT_CENTER, format!("{}-{}", t.anno, t.num), bold(22.0), TEXT);
                        let sotto = format!("{} {} · gruppo R {}-{}", t.codart, t.descr, t.anno_r, t.num_r);
                        p.text(rr.left_center() + vec2(18.0, 16.0), Align2::LEFT_CENTER, w::elide(p, &sotto, reg(16.0), 470.0), reg(16.0), MUTED);
                        p.text(rr.right_center() - vec2(56.0, 12.0), Align2::RIGHT_CENTER, format!("{} {}", dati::fmt_qta(t.qta, 3), t.um), bold(19.0), PRIMARY_DARK);
                        p.text(rr.right_center() - vec2(56.0, -14.0), Align2::RIGHT_CENTER, t.macchine.join(", "), reg(15.0), MUTED);
                        let cx = rr.right() - 26.0;
                        p.line_segment([pos2(cx - 5.0, rr.center().y - 10.0), pos2(cx + 5.0, rr.center().y)], Stroke::new(3.0, TEXT));
                        p.line_segment([pos2(cx + 5.0, rr.center().y), pos2(cx - 5.0, rr.center().y + 10.0)], Stroke::new(3.0, TEXT));
                        if resp.clicked() {
                            esito = Esito::RapidoScelto(i);
                        }
                        ui.add_space(6.0);
                    }
                });
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::tile(ui, vec2(180.0, 64.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
            Dialogo::Barcode { errore, ricerca, info } => {
                let info = *info;
                let wd = 660.0;
                ui.set_width(wd);
                // Reader mark: pulsing ring while listening, spinner while searching
                let (hr, _) = ui.allocate_exact_size(vec2(wd, 150.0), Sense::hover());
                let c = pos2(hr.center().x, hr.top() + 70.0);
                let p = ui.painter();
                let t = ctx.input(|i| i.time) as f32;
                if *ricerca {
                    p.circle_filled(c, 62.0, lerp(PRIMARY, Color32::WHITE, 0.9));
                } else {
                    let f = (t * 1.6).fract();
                    let a = ((1.0 - f) * 120.0) as u8;
                    p.circle_stroke(c, 60.0 + f * 16.0, Stroke::new(4.0, Color32::from_rgba_unmultiplied(PRIMARY.r(), PRIMARY.g(), PRIMARY.b(), a)));
                    p.circle_filled(c, 60.0, lerp(PRIMARY, Color32::WHITE, 0.88));
                }
                w::icon(p, Rect::from_center_size(c, vec2(70.0, 70.0)), Icon::Barcode, PRIMARY_DARK);
                if *ricerca {
                    ui.put(Rect::from_center_size(c + vec2(52.0, 46.0), vec2(30.0, 30.0)), egui::Spinner::new().size(30.0).color(PRIMARY));
                }
                ctx.request_repaint_after(Duration::from_millis(30));
                let titolo = match (*ricerca, info) {
                    (true, _) => "Ricerca della commessa…",
                    (false, true) => "Info commessa · lettore in ascolto",
                    (false, false) => "Lettore in ascolto",
                };
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(titolo).font(bold(26.0)).color(TEXT));
                    ui.add_space(4.0);
                    let sotto = match (in_versamento, &mac_nome) {
                        _ if info => "Spara il barcode: mostra su quale macchina di questa postazione si trova la commessa. Non registra nulla.".to_string(),
                        (true, _) => "Spara il barcode della commessa da versare in questa fase".to_string(),
                        (false, Some(m)) => format!("Spara il barcode: cerca solo le commesse di {m}, apre commessa e fase, versa e torna com'era"),
                        (false, None) => "Seleziona prima la macchina in MACCHINE".to_string(),
                    };
                    ui.add(egui::Label::new(egui::RichText::new(sotto).size(17.0).color(MUTED)).wrap());
                });
                ui.add_space(16.0);
                // What the reader is typing
                let (lr, _) = ui.allocate_exact_size(vec2(wd, 64.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(lr, 12.0, CARD_SOFT);
                p.rect_stroke(lr, 12.0, Stroke::new(1.0, BORDER_HI), StrokeKind::Inside);
                if lettura.is_empty() {
                    let txt = if *ricerca { "…" } else { "in attesa del codice" };
                    p.text(lr.center(), Align2::CENTER_CENTER, txt, reg(19.0), FAINT);
                } else {
                    p.text(lr.center(), Align2::CENTER_CENTER, &lettura, bold(28.0), PRIMARY_DARK);
                }
                if let Some(e) = errore {
                    ui.add_space(12.0);
                    egui::Frame::new()
                        .fill(lerp(DANGER, Color32::WHITE, 0.88))
                        .stroke(Stroke::new(1.0, lerp(DANGER, Color32::WHITE, 0.4)))
                        .corner_radius(12)
                        .inner_margin(Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.set_width(wd - 32.0);
                            ui.horizontal(|ui| {
                                let (ir, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                                w::icon(ui.painter(), ir, Icon::Close, DANGER);
                                ui.add(egui::Label::new(egui::RichText::new(e.as_str()).size(17.0).color(TEXT)).wrap());
                            });
                            ui.label(egui::RichText::new("Spara un altro codice o inseriscilo a mano.").size(15.0).color(MUTED));
                        });
                }
                ui.add_space(18.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if w::tile(ui, vec2(180.0, 66.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                    if w::tile(ui, vec2(260.0, 66.0), Some(Icon::Keyboard), "INSERISCI A MANO", Kind::Normal, !*ricerca).clicked() {
                        esito = Esito::BarcodeMano;
                    }
                    // batch mode: collect several orders, then one run
                    if !info && !in_versamento && w::tile(ui, vec2(220.0, 66.0), Some(Icon::Layers), "PIÙ COMMESSE", Kind::Orange, !*ricerca).clicked() {
                        esito = Esito::LottoApri;
                    }
                });
            }
            Dialogo::Lotto { voci, errore, ricerca } => {
                let mac = mac_nome.clone().unwrap_or_else(|| "la macchina".into());
                if let Some(a) = super::lotto::disegna_lotto(ui, ctx, voci, errore, *ricerca, &lettura, &mac, ndec) {
                    esito = Esito::Lotto(a);
                }
            }
            Dialogo::LottoScelta { codice, trovate } => {
                ui.set_width(800.0);
                ui.label(egui::RichText::new(format!("Codice «{codice}»: {} commesse aperte, quale aggiungere?", trovate.len())).font(bold(23.0)).color(TEXT));
                ui.add_space(10.0);
                egui::ScrollArea::vertical().max_height(430.0).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
                    for (i, t) in trovate.iter().enumerate() {
                        let (rr, resp) = ui.allocate_exact_size(vec2(780.0, 72.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(rr, 12.0, if resp.is_pointer_button_down_on() { SEL_FILL } else { CARD_SOFT });
                        p.rect_stroke(rr, 12.0, Stroke::new(1.0, BORDER_HI), StrokeKind::Inside);
                        p.text(rr.left_center() + vec2(18.0, -13.0), Align2::LEFT_CENTER, format!("{}-{}", t.anno, t.num), bold(22.0), TEXT);
                        let sotto = format!("{} {} · R {}-{}", t.codart, t.descr, t.anno_r, t.num_r);
                        p.text(rr.left_center() + vec2(18.0, 15.0), Align2::LEFT_CENTER, w::elide(p, &sotto, reg(16.0), 560.0), reg(16.0), MUTED);
                        p.text(rr.right_center() - vec2(18.0, 0.0), Align2::RIGHT_CENTER, format!("{} {}", dati::fmt_qta(t.qta, ndec), t.um), bold(19.0), PRIMARY_DARK);
                        if resp.clicked() {
                            esito = Esito::LottoScelto(i);
                        }
                        ui.add_space(6.0);
                    }
                });
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::tile(ui, vec2(180.0, 64.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
            Dialogo::InfoCommessa { codice, trovate } => {
                let wd = 900.0;
                ui.set_width(wd);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    w::icon(ui.painter(), r, Icon::Search, PRIMARY);
                    let t = if trovate.len() == 1 { format!("Info commessa · codice «{codice}»") } else { format!("Codice «{codice}»: {} commesse", trovate.len()) };
                    ui.label(egui::RichText::new(t).font(bold(23.0)).color(TEXT));
                });
                ui.label(egui::RichText::new("Solo consultazione: nulla viene registrato").size(15.0).color(MUTED));
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(470.0).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
                    for t in trovate.iter() {
                        egui::Frame::new().fill(CARD_SOFT).stroke(Stroke::new(1.0, BORDER)).corner_radius(12).inner_margin(Margin::same(16)).show(ui, |ui| {
                            ui.set_width(wd - 52.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("{}-{}", t.anno, t.num)).font(bold(26.0)).color(TEXT));
                                chip(ui, if t.chiusa { "CHIUSA" } else { "APERTA" }, if t.chiusa { MUTED } else { GOOD });
                                ui.add_space(14.0);
                                if !t.num_r.is_empty() {
                                    ui.label(egui::RichText::new(format!("nella commessa R {}-{}", t.anno_r, t.num_r)).font(bold(20.0)).color(PRIMARY_DARK));
                                    if t.r_chiusa {
                                        chip(ui, "R CHIUSA", MUTED);
                                    }
                                } else {
                                    ui.label(egui::RichText::new("senza commessa R").size(18.0).color(MUTED));
                                }
                            });
                            ui.label(egui::RichText::new(format!("{} · {}", t.codart, t.descr)).size(16.0).color(MUTED));
                            ui.label(egui::RichText::new(format!("Quantità {} {}", dati::fmt_qta(t.qta, ndec), t.um)).font(bold(17.0)).color(TEXT));
                            if !t.su_macchine.is_empty() {
                                ui.label(egui::RichText::new(format!("In lavorazione ora su: {}", t.su_macchine.join(", "))).font(bold(17.0)).color(GOOD_DARK));
                            }
                            if !t.fasi.is_empty() {
                                ui.add_space(6.0);
                                let rw = ui.available_width();
                                for f in &t.fasi {
                                    let (rr, _) = ui.allocate_exact_size(vec2(rw, 30.0), Sense::hover());
                                    let p = ui.painter();
                                    p.line_segment([rr.left_top(), rr.right_top()], Stroke::new(1.0, BORDER));
                                    let y = rr.center().y;
                                    // the machine first: that is what INFO COMMESSA is for
                                    p.text(pos2(rr.left(), y), Align2::LEFT_CENTER, w::elide(p, &f.macchina, bold(17.0), rw * 0.36), bold(17.0), PRIMARY_DARK);
                                    let fase = format!("fase {} · {}", f.progr, f.descr);
                                    p.text(pos2(rr.left() + rw * 0.38, y), Align2::LEFT_CENTER, w::elide(p, &fase, reg(16.0), rw * 0.19), reg(16.0), TEXT);
                                    let (st, col) = match f.stato {
                                        dati::StatoFase::Terminata => ("terminata", MUTED),
                                        dati::StatoFase::InCorso => ("in corso", GOOD_DARK),
                                        dati::StatoFase::Sospesa => ("sospesa · da fare", WARN),
                                        dati::StatoFase::DaFare => ("da fare", WARN),
                                    };
                                    p.text(pos2(rr.left() + rw * 0.58, y), Align2::LEFT_CENTER, st, bold(16.0), col);
                                    if f.produzione {
                                        let v = format!("versati {} su {}", dati::fmt_qta(f.versato, ndec), dati::fmt_qta(t.qta, ndec));
                                        p.text(pos2(rr.right(), y), Align2::RIGHT_CENTER, v, reg(16.0), if f.versato >= t.qta - 1e-9 && t.qta > 0.0 { GOOD_DARK } else { TEXT });
                                    }
                                }
                            }
                        });
                        ui.add_space(8.0);
                    }
                });
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if w::tile(ui, vec2(180.0, 64.0), Some(Icon::Close), "CHIUDI", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                    if w::tile(ui, vec2(240.0, 64.0), Some(Icon::Barcode), "ALTRO CODICE", Kind::Primary, true).clicked() {
                        esito = Esito::InfoAltro;
                    }
                });
            }
            Dialogo::UscitaErrore { testo } => {
                ui.set_width(640.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 23.0, lerp(DANGER, Color32::WHITE, 0.85));
                    w::icon(ui.painter(), r.shrink(12.0), Icon::Close, DANGER);
                    ui.label(egui::RichText::new("Turno non chiuso").font(bold(23.0)).color(TEXT));
                });
                ui.add_space(8.0);
                ui.add(egui::Label::new(egui::RichText::new(testo.as_str()).size(18.0).color(TEXT)).wrap());
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Puoi annullare e sistemare (es. sospendere le fasi in corso), oppure uscire lasciando l'operatore collegato.")
                            .size(16.0)
                            .color(MUTED),
                    )
                    .wrap(),
                );
                ui.add_space(18.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if w::tile(ui, vec2(230.0, 66.0), Some(Icon::Exit), "ESCI COMUNQUE", Kind::Danger, true).clicked() {
                        esito = Esito::EsciComunque;
                    }
                    if w::tile(ui, vec2(170.0, 66.0), None, "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
            Dialogo::Esci => {
                ui.set_width(600.0);
                ui.label(egui::RichText::new("Uscire dal programma?").font(bold(23.0)).color(TEXT));
                if !in_sessione.is_empty() {
                    ui.add_space(10.0);
                    let testo = format!(
                        "Prima di chiudere verrà terminato il turno di {} e rilasciato il controllo delle sue macchine.",
                        in_sessione.join(", ")
                    );
                    ui.add(egui::Label::new(egui::RichText::new(testo).size(18.0).color(MUTED)).wrap());
                }
                ui.add_space(20.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if w::tile(ui, vec2(170.0, 66.0), Some(Icon::Exit), "ESCI", Kind::Danger, true).clicked() {
                        esito = Esito::EsciApp;
                    }
                    if w::tile(ui, vec2(170.0, 66.0), None, "ANNULLA", Kind::Normal, true).clicked() {
                        esito = Esito::Chiudi;
                    }
                });
            }
        });
        match esito {
            Esito::Niente => {}
            Esito::Chiudi => {
                let chiuso = self.dialoghi.pop();
                // INFO COMMESSA cancelled while looking up: drop the lookup
                if matches!(chiuso, Some(Dialogo::Barcode { info: true, .. })) {
                    self.info = None;
                }
                // Cancelling a step of the automatic barcode flow: undo what it started
                if self.rapido.is_some() {
                    match chiuso {
                        // still searching: nothing was started, just drop the search
                        Some(Dialogo::Barcode { .. }) => self.rapido = None,
                        Some(Dialogo::RapidoScelta { .. }) => self.fine_rapido(None),
                        Some(Dialogo::ScanScelta { .. } | Dialogo::ScanConferma { .. }) if self.vers.as_ref().map(|v| v.rapido).unwrap_or(false) => {
                            self.ripristina_rapido(None)
                        }
                        _ => {}
                    }
                }
            }
            Esito::LottoApri => self.apri_lotto(),
            Esito::LottoScelto(i) => {
                if let Some(Dialogo::LottoScelta { trovate, .. }) = self.dialoghi.pop() {
                    if let Some(t) = trovate.get(i).cloned() {
                        self.lotto_scelto(t);
                    }
                }
            }
            Esito::Lotto(a) => match a {
                super::lotto::AzioneLotto::Avvia => {
                    if let Some(Dialogo::Lotto { voci, .. }) = self.dialoghi.pop() {
                        self.avvia_lotto(voci);
                    }
                }
                super::lotto::AzioneLotto::Annulla => {
                    self.dialoghi.pop();
                    self.lotto_task = None;
                }
                super::lotto::AzioneLotto::Mano => {
                    self.dialoghi.push(Dialogo::Tastierino {
                        titolo: "Codice commessa".into(),
                        valore: String::new(),
                        password: false,
                        trattino: true,
                        decimali: false,
                        scopo: Scopo::CodiceBarcode,
                    });
                }
                super::lotto::AzioneLotto::Tastiera(i) => {
                    let q = match self.dialoghi.last() {
                        Some(Dialogo::Lotto { voci, .. }) => voci.get(i).map(|v| v.qta).unwrap_or(0.0),
                        _ => 0.0,
                    };
                    let s = dati::fmt_qta(q, ndec);
                    let s = if s.contains(',') { s.trim_end_matches('0').trim_end_matches(',').to_string() } else { s };
                    self.dialoghi.push(Dialogo::Tastierino {
                        titolo: "Quantità da versare".into(),
                        valore: s,
                        password: false,
                        trattino: false,
                        decimali: ndec > 0,
                        scopo: Scopo::QtaLotto(i),
                    });
                }
            },
            Esito::BarcodeMano => {
                self.dialoghi.push(Dialogo::Tastierino {
                    titolo: "Codice commessa".into(),
                    valore: String::new(),
                    password: false,
                    trattino: true,
                    decimali: false,
                    scopo: Scopo::CodiceBarcode,
                });
            }
            Esito::RapidoScelto(i) => {
                if let Some(Dialogo::RapidoScelta { trovate, .. }) = self.dialoghi.pop() {
                    if let Some(t) = trovate.get(i).cloned() {
                        self.scegli_rapido(t);
                    }
                }
            }
            Esito::EsciApp => {
                self.dialoghi.pop();
                self.prossima_uscita();
            }
            Esito::InfoAltro => {
                self.dialoghi.pop();
                self.apri_info();
            }
            Esito::EsciComunque => {
                self.dialoghi.pop();
                self.chiudi_ora = true;
            }
            Esito::Risposta(r) => {
                if let Some(Dialogo::Domanda { code, cmd, args, dopo, .. }) = self.dialoghi.pop() {
                    self.rispondi(cmd, args, &code, &r, dopo);
                }
            }
            Esito::Chiusura(o, m, n) => {
                if let Some(Dialogo::Chiusura { cmd, mut args, dopo, .. }) = self.dialoghi.pop() {
                    args.push(("ore".into(), format!("{o}")));
                    args.push(("minuti".into(), format!("{m}")));
                    args.push(("operatori".into(), format!("{n}")));
                    self.esegui_owned(cmd, args, dopo);
                }
            }
            Esito::Valore(v) => {
                if let Some(Dialogo::Tastierino { scopo, .. }) = self.dialoghi.pop() {
                    self.valore_inserito(scopo, v);
                }
            }
            Esito::ScanScelto(idx) => {
                if let Some(Dialogo::ScanScelta { codice, .. }) = self.dialoghi.pop() {
                    if let Some(v) = &mut self.vers {
                        let qta = proposta(&v.righe[idx], v.versa_rimanenti);
                        v.evidenzia = Some(idx);
                        v.scorri = true;
                        self.dialoghi.push(Dialogo::ScanConferma { codice, idx, qta });
                    }
                }
            }
            Esito::ScanTastierino(idx, qta) => {
                let nd = righe.get(idx).map(|r| r.ndec).unwrap_or(0);
                let s = dati::fmt_qta(qta, nd);
                let s = if s.contains(',') { s.trim_end_matches('0').trim_end_matches(',').to_string() } else { s };
                self.dialoghi.push(Dialogo::Tastierino {
                    titolo: "Quantità da versare".into(),
                    valore: s,
                    password: false,
                    trattino: false,
                    decimali: nd > 0,
                    scopo: Scopo::QtaScansione(idx),
                });
            }
            Esito::ScanOk(idx, qta) => {
                self.dialoghi.pop();
                if let Some(v) = &mut self.vers {
                    if let Some(r) = v.righe.get_mut(idx) {
                        let f = 10f64.powi(r.ndec as i32);
                        r.qta = Some((qta * f).round() / f);
                        let txt = format!("Commessa {}-{}: {} pronta. Premi CONFERMA per registrare.", r.anno, r.num, dati::fmt_qta(qta, r.ndec));
                        v.evidenzia = Some(idx);
                        v.scorri = true;
                        if !v.rapido {
                            self.avvisa(txt, false);
                        }
                    }
                }
                // Automatic flow: the card's confirmation is the only one; write now
                if self.vers.as_ref().map(|v| v.rapido).unwrap_or(false) {
                    self.conferma_versamento();
                }
            }
        }
    }

    fn valore_inserito(&mut self, scopo: Scopo, v: String) {
        match scopo {
            Scopo::CodiceOperatore => {
                if v.trim().is_empty() {
                    self.messaggio("Devi inserire un codice", true);
                } else {
                    let cod = v.trim().to_string();
                    self.esegui("operatore", vec![("cod", cod.clone())], super::Dopo::Login { cod });
                }
            }
            Scopo::Password { cod } => {
                self.esegui("operatore", vec![("cod", cod.clone()), ("pwd", v)], super::Dopo::Login { cod });
            }
            Scopo::FiltroCommessa => {
                let parts: Vec<&str> = v.split('-').collect();
                match (parts.first().and_then(|a| a.trim().parse::<i32>().ok()), parts.get(1).and_then(|n| n.trim().parse::<i64>().ok())) {
                    (Some(a), Some(n)) => {
                        self.filtro = Some((a, n));
                        self.pagina = 0;
                        self.sc_comm.inizio();
                        self.carica_commesse();
                    }
                    _ => self.messaggio("Formato non valido: scrivere Anno-Numero (es. 2026-20105235)", true),
                }
            }
            Scopo::QtaScansione(idx) => {
                if let Ok(q) = v.trim().replace(',', ".").parse::<f64>() {
                    if let Some(Dialogo::ScanConferma { idx: i, qta, .. }) = self.dialoghi.last_mut() {
                        if *i == idx {
                            *qta = q.max(0.0);
                        }
                    }
                }
            }
            Scopo::ValoreIni(i) => self.valore_ini(i, v),
            Scopo::CodiceBarcode => {
                // back in the barcode window: same as a scan
                if !v.trim().is_empty() && matches!(self.dialoghi.last(), Some(Dialogo::Barcode { .. } | Dialogo::Lotto { .. })) {
                    self.barcode_letto(v);
                }
            }
            Scopo::QtaLotto(i) => {
                let ndec = self.setup.ndec;
                if let Some(Dialogo::Lotto { voci, .. }) = self.dialoghi.last_mut() {
                    super::lotto::qta_lotto(voci, i, &v, ndec);
                }
            }
            Scopo::FiltroVersamento => {
                if let Some(vers) = &mut self.vers {
                    vers.filtro = v.trim().to_string();
                }
            }
            Scopo::QtaRiga(i) => {
                if let Some(vers) = &mut self.vers {
                    let coeff = vers.coeff;
                    if let Some(r) = vers.righe.get_mut(i) {
                        if v.trim().is_empty() {
                            r.qta = None;
                        } else if let Ok(q) = v.trim().replace(',', ".").parse::<f64>() {
                            // dgPezzi_CellValueChanged: rounded to the item's decimals; a custom
                            // unit (PD_LABEL/PD_COEFFICIENTE) converts back to pieces.
                            let pezzi = match coeff {
                                Some(c) if c != 0.0 => q / c,
                                _ => q,
                            };
                            let f = 10f64.powi(r.ndec as i32);
                            r.qta = Some((pezzi * f).round() / f);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn avviso_ui(&mut self, ctx: &egui::Context) {
        let Some((t, at, err)) = &self.avviso else { return };
        if at.elapsed() > Duration::from_secs(5) {
            self.avviso = None;
            return;
        }
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("avviso")).fade_in(false)
            .order(egui::Order::Tooltip)
            .fixed_pos(pos2(screen.center().x - 320.0, screen.top() + 104.0))
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(640.0, 58.0), Sense::hover());
                let p = ui.painter();
                w::shadow(p, r, 14, 50);
                p.rect_filled(r, 14.0, if *err { DANGER } else { GOOD });
                p.text(r.center(), Align2::CENTER_CENTER, w::elide(p, t, bold(19.0), 610.0), bold(19.0), Color32::WHITE);
            });
    }
}

pub(super) fn occupato_ui(ctx: &egui::Context) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("busy")).fade_in(false).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(60));
        let c = Rect::from_center_size(r.center(), vec2(380.0, 96.0));
        w::shadow(ui.painter(), c, 16, 50);
        ui.painter().rect_filled(c, 16.0, CARD);
        ui.put(Rect::from_center_size(c.left_center() + vec2(48.0, 0.0), vec2(36.0, 36.0)), egui::Spinner::new().size(36.0).color(PRIMARY));
        ui.painter().text(c.left_center() + vec2(86.0, 0.0), Align2::LEFT_CENTER, "Elaborazione in corso…", bold(20.0), TEXT);
    });
}

fn stepper(ui: &mut Ui, label: &str, v: &mut f64, step: f64, min: f64, max: f64, dec: usize) {
    ui.horizontal(|ui| {
        let (lr, _) = ui.allocate_exact_size(vec2(230.0, 64.0), Sense::hover());
        ui.painter().text(lr.left_center(), Align2::LEFT_CENTER, label, reg(20.0), TEXT);
        if w::tile(ui, vec2(74.0, 60.0), None, "−", Kind::Normal, *v > min).clicked() {
            *v = (*v - step).max(min);
        }
        let (vr, _) = ui.allocate_exact_size(vec2(140.0, 60.0), Sense::hover());
        ui.painter().rect_filled(vr, 10.0, CARD_SOFT);
        ui.painter().text(vr.center(), Align2::CENTER_CENTER, dati::fmt_qta(*v, dec), bold(26.0), TEXT);
        if w::tile(ui, vec2(74.0, 60.0), None, "+", Kind::Normal, *v < max).clicked() {
            *v = (*v + step).min(max);
        }
    });
}

/// Letters-and-digits keyboard for operator code and password, same keys as the original's
/// FormInputStringa (RD.Logic, InitializeComponent 0x34c4): 1–0, QWERTY capitals, the decimal
/// separator of the system (.ctor 0x33bc, "," in Italian), delete (bDelete_Click 0x3434);
/// keys append their caption (bTasto_Click 0x3411); no length limit; password shown as '*'
/// (.ctor: PasswordChar = '*'). Added: "abc" for lower case on touch.
/// Some(Some(v)) = OK, Some(None) = cancel, None = still open.
fn tastiera(ui: &mut Ui, titolo: &str, valore: &mut String, password: bool, simboli: bool) -> Option<Option<String>> {
    let mut out = None;
    let k = vec2(64.0, 64.0);
    let gap = 8.0;
    let wd = k.x * 10.0 + gap * 9.0;
    let id_minuscole = egui::Id::new("tastiera-minuscole");
    let mut minuscole = ui.data(|d| d.get_temp::<bool>(id_minuscole).unwrap_or(false));
    ui.set_width(wd);
    ui.label(egui::RichText::new(titolo).font(bold(23.0)).color(TEXT));
    ui.add_space(10.0);
    // A physical keyboard or badge reader works too
    ui.input(|i| {
        for e in &i.events {
            match e {
                egui::Event::Text(t) => {
                    for ch in t.chars().filter(|c| c.is_ascii_graphic() || (simboli && *c == ' ')) {
                        valore.push(ch);
                    }
                }
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                    valore.pop();
                }
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => out = Some(Some(valore.clone())),
                egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => out = Some(None),
                _ => {}
            }
        }
    });
    let (dr, _) = ui.allocate_exact_size(vec2(wd, 70.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(dr, 12.0, CARD_SOFT);
    p.rect_stroke(dr, 12.0, Stroke::new(1.5, SEL_BORDER), StrokeKind::Inside);
    let mut shown = if password { "*".repeat(valore.chars().count()) } else { valore.clone() };
    // long values (paths, connection strings): keep the end, where the typing happens
    let font = if simboli { bold(24.0) } else { bold(32.0) };
    while shown.chars().count() > 1 && p.layout_no_wrap(format!("…{shown}"), font.clone(), TEXT).size().x > wd - 36.0 {
        shown.remove(0);
    }
    if shown.chars().count() < valore.chars().count() {
        shown.insert(0, '…');
    }
    p.text(dr.right_center() - vec2(18.0, 0.0), Align2::RIGHT_CENTER, shown, font, TEXT);
    ui.add_space(8.0);
    // '␣' = space bar (settings values only)
    let mut righe = vec!["1234567890", "QWERTYUIOP", "ASDFGHJKL,", "ZXCVBNM<"];
    if simboli {
        righe.push(".:_\\/=;-␣");
    }
    for riga in righe {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // backspace and space are two keys wide
            let n = riga.chars().map(|c| if c == '<' || c == '␣' { 2.0 } else { 1.0 }).sum::<f32>();
            ui.add_space((wd - (n * k.x + (n - 1.0) * gap)) / 2.0);
            for ch in riga.chars() {
                let resp = if ch == '<' {
                    w::tile(ui, vec2(k.x * 2.0 + gap, k.y), Some(Icon::Backspace), "", Kind::Normal, true)
                } else if ch == '␣' {
                    w::tile(ui, vec2(k.x * 2.0 + gap, k.y), None, "SPAZIO", Kind::Normal, true)
                } else {
                    let c = if minuscole { ch.to_ascii_lowercase() } else { ch };
                    w::tile(ui, k, None, &c.to_string(), Kind::Normal, true)
                };
                if resp.clicked() {
                    if ch == '<' {
                        valore.pop();
                    } else if ch == '␣' {
                        valore.push(' ');
                    } else {
                        valore.push(if minuscole { ch.to_ascii_lowercase() } else { ch });
                    }
                }
            }
        });
        ui.add_space(2.0);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let b = vec2((wd - 14.0 * 3.0) / 4.0, 70.0);
        if w::tile(ui, b, None, "ANNULLA", Kind::Normal, true).clicked() {
            out = Some(None);
        }
        if w::tile(ui, b, None, if minuscole { "ABC" } else { "abc" }, Kind::Normal, true).clicked() {
            minuscole = !minuscole;
        }
        if w::tile(ui, b, None, "C", Kind::Normal, true).clicked() {
            valore.clear();
        }
        if w::tile(ui, b, Some(Icon::Check), "OK", Kind::Good, true).clicked() {
            out = Some(Some(valore.clone()));
        }
    });
    ui.data_mut(|d| d.insert_temp(id_minuscole, minuscole));
    out
}

/// Number pad (TouchHelper.ShowTastierino / FormInputNumerico).
/// Some(Some(v)) = OK, Some(None) = cancel, None = still open.
fn tastierino(ui: &mut Ui, titolo: &str, valore: &mut String, password: bool, trattino: bool, decimali: bool) -> Option<Option<String>> {
    let mut out = None;
    ui.set_width(430.0);
    ui.label(egui::RichText::new(titolo).font(bold(23.0)).color(TEXT));
    ui.add_space(10.0);
    // A physical keyboard or barcode reader works too
    ui.input(|i| {
        for e in &i.events {
            match e {
                egui::Event::Text(t) => {
                    for ch in t.chars() {
                        if ch.is_ascii_alphanumeric() || (ch == '-' && trattino) || ((ch == ',' || ch == '.') && decimali) {
                            valore.push(if ch == '.' { ',' } else { ch });
                        }
                    }
                }
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                    valore.pop();
                }
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => out = Some(Some(valore.clone())),
                egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => out = Some(None),
                _ => {}
            }
        }
    });
    let (dr, _) = ui.allocate_exact_size(vec2(430.0, 70.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(dr, 12.0, CARD_SOFT);
    p.rect_stroke(dr, 12.0, Stroke::new(1.5, SEL_BORDER), StrokeKind::Inside);
    let shown = if password { "●".repeat(valore.chars().count()) } else { valore.clone() };
    p.text(dr.right_center() - vec2(18.0, 0.0), Align2::RIGHT_CENTER, shown, bold(32.0), TEXT);
    ui.add_space(8.0);
    let k = vec2(134.0, 76.0);
    let extra = if trattino { "-" } else if decimali { "," } else { "" };
    let rows = [["7", "8", "9"], ["4", "5", "6"], ["1", "2", "3"], [extra, "0", "<"]];
    for row in rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for key in row {
                if key.is_empty() {
                    ui.allocate_exact_size(k, Sense::hover());
                    continue;
                }
                let resp = if key == "<" { w::tile(ui, k, Some(Icon::Backspace), "", Kind::Normal, true) } else { w::tile(ui, k, None, key, Kind::Normal, true) };
                if resp.clicked() {
                    if key == "<" {
                        valore.pop();
                    } else {
                        valore.push_str(key);
                    }
                }
            }
        });
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        if w::tile(ui, vec2(134.0, 70.0), None, "ANNULLA", Kind::Normal, true).clicked() {
            out = Some(None);
        }
        if w::tile(ui, vec2(134.0, 70.0), None, "C", Kind::Normal, true).clicked() {
            valore.clear();
        }
        if w::tile(ui, vec2(134.0, 70.0), Some(Icon::Check), "OK", Kind::Good, true).clicked() {
            out = Some(Some(valore.clone()));
        }
    });
    out
}

/// Small coloured label (order state in INFO COMMESSA).
fn chip(ui: &mut Ui, testo: &str, colore: Color32) {
    let g = ui.painter().layout_no_wrap(testo.to_string(), bold(13.0), colore);
    let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 26.0), Sense::hover());
    ui.painter().rect_filled(r, 13.0, lerp(colore, Color32::WHITE, 0.85));
    ui.painter().rect_stroke(r, 13.0, Stroke::new(1.0, lerp(colore, Color32::WHITE, 0.4)), StrokeKind::Inside);
    ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, colore);
}
