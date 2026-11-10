//! The screens (one per tab) and the bottom action bar.

use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{Dialogo, Main, Scopo, Tab};
use crate::dati::{self, Commessa, Fase, Semaforo, StatoFase, Turno};
use crate::theme::*;
use crate::ui::{self as w, Icon, Kind};

/// Touch-scrolled list (drag with the finger) that the ▲/▼ buttons move by one page,
/// like the original's bMacchineSu/Giu (VerticalScroll.Value ± LargeChange).
#[derive(Default)]
pub(super) struct Scorri {
    pub offset: f32,
    pub vai: Option<f32>,
    pub pagina: f32,
    pub max: f32,
}

impl Scorri {
    pub fn su(&mut self) {
        self.vai = Some((self.offset - self.pagina).max(0.0));
    }
    pub fn giu(&mut self) {
        self.vai = Some((self.offset + self.pagina).min(self.max));
    }
    pub fn inizio(&mut self) {
        self.vai = Some(0.0);
    }
    pub fn puo_su(&self) -> bool {
        self.offset > 0.5
    }
    pub fn puo_giu(&self) -> bool {
        self.offset < self.max - 0.5
    }
    fn area(&mut self, salt: &str) -> egui::ScrollArea {
        // Drag always scrolls: egui's default (DragScroll::OnTouch) stays off when Windows
        // delivers the touch screen as mouse input.
        let mut sa = egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource::ALL);
        if let Some(o) = self.vai.take() {
            sa = sa.vertical_scroll_offset(o);
        }
        sa
    }
    fn leggi<R>(&mut self, out: &egui::scroll_area::ScrollAreaOutput<R>) {
        self.offset = out.state.offset().y;
        self.max = (out.content_size.y - out.inner_rect.height()).max(0.0);
    }
}

impl Main {
    /// Sub-tabs of COMMESSE: the order list and the phases of the running order.
    pub(super) fn sotto_schede(&mut self, ui: &mut Ui) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
        let seg = Rect::from_min_size(row.left_top() + vec2(0.0, 2.0), vec2(460.0, 42.0));
        let p = ui.painter().clone();
        p.rect_filled(seg, 21.0, CARD_SOFT);
        p.rect_stroke(seg, 21.0, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
        let voci = [(Tab::Commesse, Icon::Layers, "COMMESSE"), (Tab::Fasi, Icon::Play, "FASI")];
        for (i, (t, ic, l)) in voci.iter().enumerate() {
            let r = Rect::from_min_size(seg.left_top() + vec2(i as f32 * 230.0 + 3.0, 3.0), vec2(224.0, 36.0));
            let on = self.tab == *t || (*t == Tab::Fasi && self.tab == Tab::Produzione);
            let resp = ui.interact(r, ui.id().with(("sub", i)), Sense::click());
            if on {
                p.rect_filled(r, 18.0, PRIMARY);
            }
            let fg = if on { Color32::WHITE } else { MUTED };
            let galley = p.layout_no_wrap(l.to_string(), bold(17.0), fg);
            let x0 = r.center().x - (galley.size().x + 28.0) / 2.0;
            w::icon(&p, Rect::from_min_size(pos2(x0, r.center().y - 10.0), vec2(20.0, 20.0)), *ic, fg);
            p.galley(pos2(x0 + 28.0, r.center().y - galley.size().y / 2.0), galley, fg);
            if resp.clicked() && self.tab != *t {
                self.tab = *t;
                match t {
                    Tab::Fasi => self.carica_ambiente(),
                    _ => self.carica_commesse(),
                }
            }
        }
        if self.tab == Tab::Produzione {
            p.text(seg.right_center() + vec2(18.0, 0.0), Align2::LEFT_CENTER, "›  Pezzi registrati", bold(18.0), TEXT);
        }
    }

    pub(super) fn barra_azioni(&mut self, ui: &mut Ui) {
        let h = ui.available_height();
        // Fit the widest bar (orders: 5 actions + 5 navigation buttons) on 1180 px and up
        let bw = ((ui.available_width() - 4.0 * 84.0 - 118.0 - 11.0 * 12.0 - 8.0 - 20.0) / 5.0).clamp(118.0, 200.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            match self.tab {
                Tab::Commesse => {
                    if w::tile(ui, vec2(bw + 20.0, h), Some(Icon::Play), "INIZIA", Kind::Good, true).clicked() {
                        self.inizia_commessa();
                    }
                    if w::tile(ui, vec2(bw, h), Some(Icon::Pause), "SOSPENDI", Kind::Normal, true).clicked() {
                        self.sospendi_commessa();
                    }
                    let progr = self.amb.value.as_ref().and_then(|a| a.progr.parse::<i64>().ok());
                    if stop_tile(ui, vec2(bw, h), "TERMINA").clicked() {
                        // bTerminaCommessa_Click_1 = ChiudiSospendiFase('T') on the phase in use
                        self.chiudi_fase("T", progr);
                    }
                    sep(ui, h);
                    if w::tile(ui, vec2(bw, h), Some(Icon::Doc), "DETTAGLI", Kind::Normal, true).clicked() {
                        self.tab = Tab::Articoli;
                        self.carica_articoli();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Refresh), "", Kind::Normal, true).clicked() {
                            self.carica_commesse();
                        }
                        let n = self.commesse.value.as_ref().map(|v| v.len()).unwrap_or(0);
                        let per = self.post.commesse_per_blocco.max(1);
                        let pagine = n.div_ceil(per).max(1);
                        let (rr, _) = ui.allocate_exact_size(vec2(118.0, h), Sense::hover());
                        ui.painter().text(rr.center(), Align2::CENTER_CENTER, format!("Pag.  {} / {}", self.pagina + 1, pagine), bold(21.0), TEXT);
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Down), "", Kind::Normal, self.sc_comm.puo_giu()).clicked() {
                            self.sc_comm.giu();
                        }
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Up), "", Kind::Normal, self.sc_comm.puo_su()).clicked() {
                            self.sc_comm.su();
                        }
                        let filtro_on = self.filtro.is_some();
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Filter), "", if filtro_on { Kind::Good } else { Kind::Normal }, true).clicked() {
                            // chFiltroComme_Click
                            if filtro_on {
                                self.filtro = None;
                                self.sc_comm.inizio();
                                self.carica_commesse();
                            } else {
                                self.dialoghi.push(Dialogo::Tastierino {
                                    titolo: "Anno-Numero".into(),
                                    valore: String::new(),
                                    password: false,
                                    trattino: true,
                                    decimali: false,
                                    scopo: Scopo::FiltroCommessa,
                                });
                            }
                        }
                    });
                }
                Tab::Macchine => {
                    let (su, giu) = (self.sc_mac.puo_su(), self.sc_mac.puo_giu());
                    // PRENDI MACCHINA: the logged-in operator takes the selected machine. Caption
                    // from LABEL_CONTROLLO_MACCHINA when set (ESP_FormMain::.ctor 0x1a054, MG: empty).
                    let etichetta = if self.setup.label_controllo.is_empty() { "PRENDI MACCHINA".to_string() } else { self.setup.label_controllo.to_uppercase() };
                    if w::tile(ui, vec2(bw + 100.0, h), Some(Icon::Pin), &etichetta, Kind::Primary, self.mac.is_some()).clicked() {
                        self.controlla_macchina();
                    }
                    let chi = self.accesso.as_ref().map(|a| if a.1.trim().is_empty() { a.0.clone() } else { a.1.trim().to_string() }).unwrap_or_default();
                    let dove = self.macchina().map(|m| m.descr).unwrap_or_else(|| "tocca una macchina".into());
                    let (tr, _) = ui.allocate_exact_size(vec2(440.0, h), Sense::hover());
                    ui.painter().text(tr.left_center() + vec2(6.0, -12.0), Align2::LEFT_CENTER, w::elide(ui.painter(), &format!("Operatore: {chi}"), bold(18.0), 430.0), bold(18.0), TEXT);
                    ui.painter().text(tr.left_center() + vec2(6.0, 14.0), Align2::LEFT_CENTER, w::elide(ui.painter(), &format!("Macchina: {dove}"), reg(16.0), 430.0), reg(16.0), MUTED);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Refresh), "", Kind::Normal, true).clicked() {
                            self.carica_macchine();
                        }
                        // bMacchineGiu_Click / bMacchineSu_Click (the list also scrolls with the finger)
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Down), "", Kind::Normal, giu).clicked() {
                            self.sc_mac.giu();
                        }
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Up), "", Kind::Normal, su).clicked() {
                            self.sc_mac.su();
                        }
                    });
                }
                Tab::Operatori => {
                    if w::tile(ui, vec2(bw + 40.0, h), Some(Icon::Login), "INIZIA TURNO", Kind::Good, true).clicked() {
                        self.inizia_turno();
                    }
                    if w::tile(ui, vec2(bw + 40.0, h), Some(Icon::Logout), "FINE TURNO", Kind::Danger, true).clicked() {
                        self.fine_turno();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Refresh), "", Kind::Normal, true).clicked() {
                            self.carica_turni();
                        }
                    });
                }
                Tab::Fasi => {
                    if w::tile(ui, vec2(bw + 20.0, h), Some(Icon::Play), "INIZIA FASE", Kind::Good, true).clicked() {
                        self.inizia_fase();
                    }
                    if w::tile(ui, vec2(bw + 20.0, h), Some(Icon::Pause), "SOSPENDI FASE", Kind::Normal, true).clicked() {
                        self.chiudi_fase("R", None);
                    }
                    if stop_tile(ui, vec2(bw + 20.0, h), "TERMINA FASE").clicked() {
                        self.chiudi_fase("T", None);
                    }
                    sep(ui, h);
                    if w::tile(ui, vec2(bw + 20.0, h), Some(Icon::Pin), "UTILIZZA FASE", Kind::Normal, true).clicked() {
                        self.utilizza_fase();
                    }
                    if w::tile(ui, vec2(bw + 20.0, h), Some(Icon::Cube), "VERSA PEZZI", Kind::Primary, true).clicked() {
                        self.versa_pezzi();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Refresh), "", Kind::Normal, true).clicked() {
                            self.carica_ambiente();
                        }
                        if w::tile(ui, vec2(bw + 10.0, h), Some(Icon::Doc), "PEZZI REGISTRATI", Kind::Normal, true).clicked() {
                            self.tab = Tab::Produzione;
                            self.carica_versati();
                        }
                    });
                }
                Tab::Produzione => {
                    if w::tile(ui, vec2(bw + 20.0, h), Some(Icon::Play), "TORNA ALLE FASI", Kind::Normal, true).clicked() {
                        self.tab = Tab::Fasi;
                    }
                    if w::tile(ui, vec2(bw + 60.0, h), Some(Icon::Cube), "VERSA PEZZI", Kind::Primary, true).clicked() {
                        self.versa_pezzi();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Refresh), "", Kind::Normal, true).clicked() {
                            self.carica_versati();
                        }
                    });
                }
                Tab::Articoli => {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::tile(ui, vec2(84.0, h), Some(Icon::Refresh), "", Kind::Normal, true).clicked() {
                            self.carica_articoli()
                        }
                    });
                }
            }
        });
    }

    pub(super) fn vista_commesse(&mut self, ui: &mut Ui) {
        let loading = self.commesse.loading();
        let Some(list) = self.commesse.value.clone() else {
            vuoto(ui, if loading { "Caricamento commesse…" } else { "Selezionare una macchina" }, loading);
            return;
        };
        if list.is_empty() {
            vuoto(ui, if loading { "Caricamento commesse…" } else { "Nessuna commessa da lavorare su questa macchina" }, loading);
            return;
        }
        // BloccoCommesse cards fill the screen; the whole list scrolls with the finger and
        // ▲/▼ move by one block (the original pages by BloccoCommesse).
        let per = self.post.commesse_per_blocco.max(1);
        let gap = 12.0;
        let avail = ui.available_rect_before_wrap();
        let card_h = ((avail.height() - gap * (per as f32 - 1.0)) / per as f32).clamp(88.0, 170.0);
        let passo = card_h + gap;
        self.sc_comm.pagina = per as f32 * passo;
        let ndec = self.setup.ndec;
        let dt = self.setup.show_dt_consegna;
        let mut scelta = None;
        let sel_now = self.sel_comm.clone();
        let out = self.sc_comm.area("commesse").show(ui, |ui| {
            let origin = ui.cursor().min;
            let wd = ui.available_width() - 14.0;
            for (i, c) in list.iter().enumerate() {
                let rect = Rect::from_min_size(origin + vec2(0.0, i as f32 * passo), vec2(wd, card_h));
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let sel = sel_now.as_ref() == Some(&c.chiave());
                if w::card(ui, rect, sel, ui.id().with(("comm", &c.anno, &c.num))).clicked() {
                    scelta = Some(c.chiave());
                }
                disegna_commessa(ui, rect, c, ndec, dt, sel);
            }
            ui.allocate_space(vec2(wd, list.len() as f32 * passo - gap));
        });
        self.sc_comm.leggi(&out);
        // page shown in the bar: the block at the top of the list (last one at the end)
        let pagine = list.len().div_ceil(per).max(1);
        self.pagina = if self.sc_comm.max > 0.0 && self.sc_comm.offset >= self.sc_comm.max - 0.5 {
            pagine - 1
        } else {
            ((self.sc_comm.offset / self.sc_comm.pagina.max(1.0)).round() as usize).min(pagine - 1)
        };
        if let Some(k) = scelta {
            self.sel_comm = Some(k);
        }
    }

    pub(super) fn vista_macchine(&mut self, ui: &mut Ui) {
        let list = self.macchine.value.clone().unwrap_or_default();
        if list.is_empty() {
            vuoto(ui, "Nessuna macchina configurata per questa postazione (Esprit.ini, MACCHINE)", self.macchine.loading());
            return;
        }
        let mut clicked = None;
        let out = self.sc_mac.area("macchine").show(ui, |ui| {
            let cols = 2;
            let gap = 14.0;
            let wd = (ui.available_width() - gap) / cols as f32;
            let h = 150.0;
            let origin = ui.cursor().min;
            for (i, m) in list.iter().enumerate() {
                let rect = Rect::from_min_size(origin + vec2((i % cols) as f32 * (wd + gap), (i / cols) as f32 * (h + gap)), vec2(wd, h));
                let sel = self.mac.as_ref() == Some(&m.cod);
                if w::card(ui, rect, sel, ui.id().with(("mac", &m.cod))).clicked() {
                    clicked = Some(m.cod.clone());
                }
                let p = ui.painter();
                w::icon(p, Rect::from_min_size(rect.left_top() + vec2(22.0, 24.0), vec2(44.0, 44.0)), Icon::Factory, if sel { PRIMARY } else { MUTED });
                p.text(rect.left_top() + vec2(84.0, 20.0), Align2::LEFT_TOP, w::elide(p, &m.descr, bold(24.0), wd - 160.0), bold(24.0), TEXT);
                p.text(rect.left_top() + vec2(84.0, 54.0), Align2::LEFT_TOP, &m.cod, reg(16.0), MUTED);
                w::semaforo(p, rect.right_top() + vec2(-40.0, 40.0), 15.0, if m.anno.is_empty() { Semaforo::Rosso } else { Semaforo::Verde });
                let col = (wd - 44.0) / 3.0;
                let info = [
                    ("OPERATORE", if m.operatore_nome.is_empty() { m.operatore.clone() } else { m.operatore_nome.clone() }),
                    ("COMMESSA", m.commessa_label()),
                    ("FASE", if m.progr.is_empty() { String::new() } else { format!("{} - {}", m.progr, m.fase_descr) }),
                ];
                for (j, (l, v)) in info.iter().enumerate() {
                    let v = if v.trim().is_empty() { "—".to_string() } else { v.clone() };
                    w::label_value(p, pos2(rect.left() + 22.0 + j as f32 * col, rect.top() + 90.0), l, &v, 18.0, TEXT, col - 12.0);
                }
            }
            let rows = list.len().div_ceil(cols);
            ui.allocate_space(vec2(ui.available_width(), rows as f32 * (h + gap)));
        });
        self.sc_mac.leggi(&out);
        // one row of cards per ▲/▼ press
        self.sc_mac.pagina = 150.0 + 14.0;
        if let Some(c) = clicked {
            self.seleziona_macchina(c);
        }
    }

    pub(super) fn vista_operatori(&mut self, ui: &mut Ui) {
        let list: Vec<Turno> = self.turni.value.clone().unwrap_or_default().into_iter().filter(|t| self.post.operatore_ammesso(&t.cod)).collect();
        if list.is_empty() {
            let loading = self.turni.loading();
            vuoto(ui, if loading { "Caricamento…" } else { "Nessun operatore in turno. Premere INIZIA TURNO." }, loading);
            return;
        }
        let mut clicked = None;
        let amb_op = self.amb.value.as_ref().map(|a| a.cod_dip.clone()).unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
            let cols = 3;
            let gap = 14.0;
            let wd = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
            let h = 128.0;
            let origin = ui.cursor().min;
            for (i, t) in list.iter().enumerate() {
                let rect = Rect::from_min_size(origin + vec2((i % cols) as f32 * (wd + gap), (i / cols) as f32 * (h + gap)), vec2(wd, h));
                let sel = self.sel_op.as_ref() == Some(&t.cod);
                if w::card(ui, rect, sel, ui.id().with(("op", &t.cod))).clicked() {
                    clicked = Some(t.cod.clone());
                }
                let p = ui.painter();
                let c = rect.left_top() + vec2(48.0, 48.0);
                p.circle_filled(c, 30.0, if sel { PRIMARY } else { CARD_SOFT });
                w::icon(p, Rect::from_center_size(c, vec2(34.0, 34.0)), Icon::Person, if sel { Color32::WHITE } else { PRIMARY });
                p.text(rect.left_top() + vec2(92.0, 22.0), Align2::LEFT_TOP, w::elide(p, &t.nome, bold(21.0), wd - 110.0), bold(21.0), TEXT);
                p.text(rect.left_top() + vec2(92.0, 52.0), Align2::LEFT_TOP, &t.cod, reg(16.0), MUTED);
                let dal = t.inizio.map(|d| format!("In turno dal {}", d.format("%d/%m %H:%M"))).unwrap_or_default();
                w::icon(p, Rect::from_min_size(rect.left_top() + vec2(22.0, 92.0), vec2(20.0, 20.0)), Icon::Clock, MUTED);
                p.text(rect.left_top() + vec2(50.0, 102.0), Align2::LEFT_CENTER, dal, reg(16.0), MUTED);
                if t.cod == amb_op {
                    p.text(rect.right_bottom() + vec2(-18.0, -24.0), Align2::RIGHT_CENTER, "SU QUESTA MACCHINA", bold(13.0), GOOD);
                }
            }
            let rows = list.len().div_ceil(cols);
            ui.allocate_space(vec2(ui.available_width(), rows as f32 * (h + gap)));
        });
        if let Some(c) = clicked {
            self.sel_op = Some(c);
        }
    }

    pub(super) fn vista_fasi(&mut self, ui: &mut Ui) {
        let amb = self.amb.value.clone().unwrap_or_default();
        if amb.anno.is_empty() {
            vuoto(ui, "Nessuna commessa avviata su questa macchina. Scegliere una commessa e premere INIZIA.", self.amb.loading());
            return;
        }
        let list = self.fasi.value.clone().unwrap_or_default();
        if list.is_empty() {
            let loading = self.fasi.loading();
            vuoto(ui, if loading { "Caricamento fasi…" } else { "Nessuna fase su questa macchina" }, loading);
            return;
        }
        let (br, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
        let p = ui.painter();
        let t = format!("Commessa {}", amb.commessa_label());
        let tr = p.text(br.left_center(), Align2::LEFT_CENTER, t, bold(22.0), TEXT);
        p.text(pos2(tr.right() + 24.0, br.center().y), Align2::LEFT_CENTER, w::elide(p, &amb.articolo, reg(18.0), br.right() - tr.right() - 30.0), reg(18.0), MUTED);
        let ndec = self.setup.ndec;
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
            let h = 132.0;
            let gap = 12.0;
            let origin = ui.cursor().min;
            let wd = ui.available_width();
            for (i, f) in list.iter().enumerate() {
                let rect = Rect::from_min_size(origin + vec2(0.0, i as f32 * (h + gap)), vec2(wd, h));
                let sel = self.sel_fase == Some(f.progr);
                if w::card(ui, rect, sel, ui.id().with(("fase", f.progr))).clicked() {
                    clicked = Some(f.progr);
                }
                disegna_fase(ui, rect, f, ndec, amb.progr == f.progr.to_string());
            }
            ui.allocate_space(vec2(wd, list.len() as f32 * (h + gap)));
        });
        if let Some(c) = clicked {
            self.sel_fase = Some(c);
        }
    }

    pub(super) fn vista_produzione(&mut self, ui: &mut Ui) {
        let amb = self.amb.value.clone().unwrap_or_default();
        if amb.num.is_empty() || amb.progr.is_empty() {
            vuoto(ui, "Per vedere e registrare le quantità serve una fase in corso su questa macchina.", self.amb.loading());
            return;
        }
        let (ultima, rows) = self.versati.value.clone().unwrap_or((false, Vec::new()));
        let (br, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
        let p = ui.painter();
        let titolo = if ultima { "Quantità prodotta · versamento prodotti finiti" } else { "Quantità prodotta · avanzamento fase" };
        p.text(br.left_center(), Align2::LEFT_CENTER, titolo, bold(22.0), TEXT);
        p.text(br.right_center(), Align2::RIGHT_CENTER, format!("Commessa {}  ·  Fase {} {}", amb.commessa_label(), amb.progr, amb.fase), reg(17.0), MUTED);
        let loading = self.versati.loading();
        tabella(
            ui,
            &["N. reg.", "Commessa", "Articolo", "Quantità", "U.M.", "Data", "Ora", "Operatore"],
            &[0.08, 0.15, 0.25, 0.1, 0.06, 0.11, 0.07, 0.18],
            rows.iter()
                .map(|r| {
                    vec![
                        r.num_reg.clone(),
                        r.commessa.clone(),
                        r.articolo.clone(),
                        r.qta.clone(),
                        r.um.clone(),
                        r.quando.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default(),
                        r.quando.map(|d| d.format("%H:%M").to_string()).unwrap_or_default(),
                        r.operatore.clone(),
                    ]
                })
                .collect(),
            if loading { "Caricamento…" } else { "Nessuna quantità registrata per questa fase" },
        );
    }

    pub(super) fn vista_articoli(&mut self, ui: &mut Ui) {
        let amb = self.amb.value.clone().unwrap_or_default();
        if amb.num.is_empty() {
            vuoto(ui, "Nessuna commessa avviata su questa macchina.", self.amb.loading());
            return;
        }
        let rows = self.articoli.value.clone().unwrap_or_default();
        let (br, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
        ui.painter().text(br.left_center(), Align2::LEFT_CENTER, format!("Articoli da produrre · commessa {}", amb.commessa_label()), bold(22.0), TEXT);
        let ndec = self.setup.ndec;
        let loading = self.articoli.loading();
        tabella(
            ui,
            &["Commessa", "Articolo", "Descrizione", "Quantità", "U.M.", "Rif. cliente"],
            &[0.14, 0.2, 0.3, 0.1, 0.06, 0.2],
            rows.iter()
                .map(|r| vec![r.commessa.clone(), r.codart.clone(), r.descr.clone(), dati::fmt_qta(r.qta, ndec), r.um.clone(), r.rif_cliente.clone()])
                .collect(),
            if loading { "Caricamento…" } else { "Nessun articolo" },
        );
    }

}

// ---------------------------------------------------------------------------

pub(super) fn stop_tile(ui: &mut Ui, size: egui::Vec2, label: &str) -> egui::Response {
    let resp = w::tile(ui, size, None, "", Kind::Normal, true);
    let r = resp.rect;
    let p = ui.painter();
    let isz = (size.y * 0.30).min(30.0);
    p.rect_filled(Rect::from_center_size(pos2(r.center().x, r.top() + size.y * 0.36), vec2(isz, isz)), 4.0, DANGER);
    p.text(pos2(r.center().x, r.top() + size.y * 0.74), Align2::CENTER_CENTER, label, bold(19.0), TEXT);
    resp
}

fn sep(ui: &mut Ui, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, h), Sense::hover());
    w::vsep(ui.painter(), r.center().x, r.top() + 8.0, r.bottom() - 8.0);
}

fn vuoto(ui: &mut Ui, testo: &str, loading: bool) {
    let r = ui.available_rect_before_wrap();
    if loading {
        ui.put(Rect::from_center_size(r.center() - vec2(0.0, 30.0), vec2(40.0, 40.0)), egui::Spinner::new().size(40.0).color(PRIMARY));
    } else {
        w::icon(ui.painter(), Rect::from_center_size(r.center() - vec2(0.0, 40.0), vec2(56.0, 56.0)), Icon::Layers, FAINT);
    }
    ui.painter().text(r.center() + vec2(0.0, 24.0), Align2::CENTER_CENTER, testo, reg(20.0), MUTED);
    ui.allocate_rect(r, Sense::hover());
}

fn disegna_commessa(ui: &Ui, rect: Rect, c: &Commessa, ndec: usize, dt_consegna: bool, sel: bool) {
    let p = ui.painter();
    let h = rect.height();
    let pad = 14.0;
    // Order code block
    let code_w = (rect.width() * 0.215).clamp(250.0, 340.0);
    let cb = Rect::from_min_size(rect.left_top() + vec2(pad, pad), vec2(code_w, h - 2.0 * pad));
    p.rect_filled(cb, 10.0, if sel { lerp(SEL_FILL, PRIMARY, 0.12) } else { CARD_SOFT });
    let code_font = bold(if code_w > 300.0 { 30.0 } else { 26.0 });
    if c.annotazione.trim().is_empty() {
        p.text(cb.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, c.codice(), code_font, TEXT);
    } else {
        p.text(cb.left_center() + vec2(16.0, -14.0), Align2::LEFT_CENTER, c.codice(), code_font, TEXT);
        p.text(cb.left_center() + vec2(16.0, 20.0), Align2::LEFT_CENTER, w::elide(p, c.annotazione.trim(), reg(16.0), code_w - 24.0), reg(16.0), MUTED);
    }
    let x1 = cb.right() + 16.0;
    w::vsep(p, x1, rect.top() + 18.0, rect.bottom() - 18.0);
    w::semaforo(p, pos2(x1 + 36.0, rect.center().y), 20.0, c.semaforo);
    // Figures on the right
    let fig_w = (rect.width() * 0.17).clamp(170.0, 260.0);
    let chevron_w = 44.0;
    let prod_x = rect.right() - chevron_w - fig_w;
    let qta_x = prod_x - fig_w;
    // Middle text
    let mx = x1 + 74.0;
    let mw = qta_x - mx - 16.0;
    let titolo = if c.descr_art.trim().is_empty() || c.descr_art.trim() == c.codart.trim() { c.codart.clone() } else { format!("{} - {}", c.codart, c.descr_art.trim()) };
    let lines = if c.note.is_empty() { 2.0 } else { 3.0 };
    let lh = (h - 2.0 * pad) / lines;
    let y0 = rect.top() + pad + lh / 2.0;
    p.text(pos2(mx, y0), Align2::LEFT_CENTER, w::elide(p, &titolo, bold(21.0), mw), bold(21.0), TEXT);
    let (dl, dv) = if dt_consegna { ("Dt. consegna", c.dt_consegna) } else { ("Inizio prev.", c.datcom) };
    w::icon(p, Rect::from_center_size(pos2(mx + 11.0, y0 + lh), vec2(20.0, 20.0)), Icon::Calendar, MUTED);
    p.text(pos2(mx + 32.0, y0 + lh), Align2::LEFT_CENTER, format!("{dl}   {}", dv.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()), reg(18.0), MUTED);
    if !c.note.is_empty() {
        w::icon(p, Rect::from_center_size(pos2(mx + 11.0, y0 + 2.0 * lh), vec2(20.0, 20.0)), Icon::Tag, MUTED);
        p.text(pos2(mx + 32.0, y0 + 2.0 * lh), Align2::LEFT_CENTER, w::elide(p, &c.note, reg(18.0), mw - 32.0), reg(18.0), MUTED);
    }
    // Q.tà / Prodotta
    let um = if c.um.trim().is_empty() { String::new() } else { format!(" {}", c.um.trim()) };
    let figure = |x: f32, ic: Icon, l: &str, v: String, extra: Option<String>| {
        w::vsep(p, x, rect.top() + 18.0, rect.bottom() - 18.0);
        w::icon(p, Rect::from_center_size(pos2(x + 34.0, rect.center().y), vec2(34.0, 34.0)), ic, PRIMARY);
        p.text(pos2(x + 64.0, rect.center().y - 14.0), Align2::LEFT_CENTER, l, reg(16.0), MUTED);
        p.text(pos2(x + 64.0, rect.center().y + 14.0), Align2::LEFT_CENTER, w::elide(p, &v, bold(25.0), fig_w - 72.0), bold(25.0), PRIMARY_DARK);
        if let Some(e) = extra {
            p.text(pos2(x + 64.0, rect.center().y + 40.0), Align2::LEFT_CENTER, w::elide(p, &e, reg(13.0), fig_w - 72.0), reg(13.0), MUTED);
        }
    };
    figure(qta_x, Icon::Layers, "Q.tà", format!("{}{}", dati::fmt_qta(c.qta, ndec), um), None);
    let tot = if (c.prodotti - c.prodotti_tot).abs() > 1e-9 { Some(format!("Totale quantità prodotta: {}", dati::fmt_qta(c.prodotti_tot, ndec))) } else { None };
    figure(prod_x, Icon::Cube, "Prodotta", format!("{}{}", dati::fmt_qta(c.prodotti, ndec), um), tot);
    // Chevron
    let cx = rect.right() - chevron_w / 2.0 - 4.0;
    let cy = rect.center().y;
    p.line_segment([pos2(cx - 6.0, cy - 12.0), pos2(cx + 6.0, cy)], Stroke::new(3.0, TEXT));
    p.line_segment([pos2(cx + 6.0, cy), pos2(cx - 6.0, cy + 12.0)], Stroke::new(3.0, TEXT));
}

fn disegna_fase(ui: &Ui, rect: Rect, f: &Fase, ndec: usize, in_uso: bool) {
    let p = ui.painter();
    let badge = Rect::from_min_size(rect.left_top() + vec2(16.0, 16.0), vec2(96.0, rect.height() - 32.0));
    p.rect_filled(badge, 10.0, CARD_SOFT);
    p.text(badge.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, "FASE", reg(14.0), MUTED);
    p.text(badge.center() + vec2(0.0, 14.0), Align2::CENTER_CENTER, f.progr.to_string(), bold(30.0), TEXT);
    w::semaforo(p, pos2(badge.right() + 36.0, rect.center().y), 18.0, f.semaforo);
    let x = badge.right() + 72.0;
    let left_w = rect.width() * 0.5 - (x - rect.left());
    p.text(pos2(x, rect.top() + 34.0), Align2::LEFT_CENTER, w::elide(p, f.descr.trim(), bold(23.0), left_w), bold(23.0), TEXT);
    let (fill, fg) = match f.stato {
        StatoFase::InCorso => (lerp(GOOD, Color32::WHITE, 0.82), GOOD),
        StatoFase::Sospesa => (lerp(WARN, Color32::WHITE, 0.8), WARN),
        StatoFase::Terminata => (CARD_SOFT, MUTED),
        StatoFase::DaFare => (lerp(PRIMARY, Color32::WHITE, 0.86), PRIMARY),
    };
    let pill = Rect::from_min_size(pos2(x, rect.top() + 58.0), vec2(150.0, 32.0));
    p.rect_filled(pill, 16.0, fill);
    p.text(pill.center(), Align2::CENTER_CENTER, f.stato.nome().to_uppercase(), bold(15.0), fg);
    if in_uso {
        p.text(pill.right_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "IN USO SULLA MACCHINA", bold(14.0), GOOD);
    }
    if let Some(oi) = f.ora_inizio {
        p.text(pos2(x, rect.top() + 110.0), Align2::LEFT_CENTER, format!("Inizio {}", oi.format("%d/%m/%Y %H:%M")), reg(16.0), MUTED);
    }
    let cols = [
        ("Prodotti", if f.produzione() { dati::fmt_qta(f.pezzi_prodotti, ndec) } else { "—".into() }),
        ("Mancanti", if f.produzione() { dati::fmt_qta(f.pezzi_mancanti, ndec) } else { "—".into() }),
        ("Tempo teorico", f.durata_teorica.map(dati::fmt_durata).unwrap_or_else(|| "—".into())),
        ("Tempo previsto", f.durata_prev.map(dati::fmt_durata).unwrap_or_else(|| "—".into())),
    ];
    let cw = (rect.width() * 0.48) / 4.0;
    let x0 = rect.right() - cw * 4.0 - 10.0;
    for (i, (l, v)) in cols.iter().enumerate() {
        let cx = x0 + i as f32 * cw;
        w::vsep(p, cx - 10.0, rect.top() + 22.0, rect.bottom() - 22.0);
        w::label_value(p, pos2(cx + 6.0, rect.center().y - 22.0), l, v, 25.0, PRIMARY_DARK, cw - 20.0);
    }
}

fn tabella(ui: &mut Ui, head: &[&str], widths: &[f32], rows: Vec<Vec<String>>, vuota: &str) {
    let wd = ui.available_width();
    let (hr, _) = ui.allocate_exact_size(vec2(wd, 44.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(hr, 10.0, CARD_SOFT);
    let mut x = hr.left() + 16.0;
    for (i, h) in head.iter().enumerate() {
        p.text(pos2(x, hr.center().y), Align2::LEFT_CENTER, *h, bold(16.0), MUTED);
        x += widths[i] * (wd - 32.0);
    }
    if rows.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.label(egui::RichText::new(vuota).size(19.0).color(MUTED)));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (n, r) in rows.iter().enumerate() {
            let (rr, _) = ui.allocate_exact_size(vec2(wd, 52.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rr, 8.0, if n % 2 == 0 { CARD } else { BG_TOP });
            let mut x = rr.left() + 16.0;
            for (i, v) in r.iter().enumerate() {
                let cw = widths[i] * (wd - 32.0);
                p.text(pos2(x, rr.center().y), Align2::LEFT_CENTER, w::elide(p, v, reg(18.0), cw - 12.0), reg(18.0), TEXT);
                x += cw;
            }
        }
    });
}
