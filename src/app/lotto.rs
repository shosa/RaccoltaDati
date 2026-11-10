//! PIÙ COMMESSE (batch of the automatic barcode flow, person's request).
//! Scans are only verified (reads: the order is open on the machine being controlled and
//! still has pieces to record on the phase the flow would use) and listed with the
//! quantity preset to the most that can still be recorded. AVVIA then runs the automatic
//! flow once per group order (R): open order and phase, write all of that group's orders
//! in ONE quantity save (ESP_FormPezziRag accepts several rows), put the machine back.

use eframe::egui::{self, Align2, Color32, Margin, Rect, Sense, Stroke, StrokeKind, Ui, vec2};

use super::rapido::{Passo, Rapido, fase_bersaglio};
use super::{Dialogo, Main};
use crate::dati::{self, Macchina, Setup, Trovata};
use crate::theme::*;
use crate::ui::{self as w, Icon, Kind};

#[derive(Clone, Debug)]
pub(super) struct VoceLotto {
    pub t: Trovata,
    /// Phase the flow will use and the most that can still be recorded there.
    pub progr: i64,
    pub max: f64,
    pub qta: f64,
    /// After the run: Ok(pieces written) or the reason it was not written.
    pub esito: Option<Result<f64, String>>,
}

impl VoceLotto {
    pub fn codice(&self) -> String {
        format!("{}-{}", self.t.anno, self.t.num)
    }
}

pub(super) enum EsitoLotto {
    Una(VoceLotto),
    Scelta(Vec<Trovata>),
    Errore(String),
}

/// What the operator did in the list window.
pub(super) enum AzioneLotto {
    Avvia,
    Annulla,
    Mano,
    Tastiera(usize),
}

/// Reads only: machine state, the group's phases on the machine, pieces already recorded.
async fn verifica(db: &crate::db::Db, az: &str, setup: &Setup, macchina: &Macchina, t: Trovata, ins_parziale: bool) -> EsitoLotto {
    let res: Result<EsitoLotto, String> = async {
        let amb = dati::ambiente(db, az, &macchina.cod).await?;
        let nome_mac = macchina.descr.trim();
        // The run would stop on these, so refuse at the scan (the machine is put back as it
        // was after each group): bNewPezzi_Click 0x220bc needs an operator (174);
        // CheckInizioCommessa 0x1e318 refuses another order while one is on the machine (169).
        if amb.cod_dip.trim().is_empty() {
            return Ok(EsitoLotto::Errore(format!("{}-{}: nessun operatore su {nome_mac} (PRENDI MACCHINA).", t.anno, t.num)));
        }
        if !amb.anno.is_empty() && !(amb.anno == t.anno_r && amb.num == t.num_r) {
            return Ok(EsitoLotto::Errore(format!(
                "{}-{} è della commessa R {}-{}, ma {nome_mac} sta lavorando la {}: si possono aggiungere solo le sue commesse, oppure terminala/sospendila prima.",
                t.anno,
                t.num,
                t.anno_r,
                t.num_r,
                amb.commessa_label()
            )));
        }
        let mut amb_r = amb.clone();
        if !(amb.anno == t.anno_r && amb.num == t.num_r) {
            amb_r.progr = String::new();
        }
        amb_r.anno = t.anno_r.clone();
        amb_r.num = t.num_r.clone();
        amb_r.tipo = "R".into();
        let fasi = dati::fasi(db, az, setup, &amb_r, &macchina.cod, ins_parziale).await?;
        let Some(progr) = fase_bersaglio(&fasi) else {
            return Ok(EsitoLotto::Errore(format!("{}-{}: nessuna fase di produzione aperta su {}.", t.anno, t.num, macchina.descr.trim())));
        };
        let versato = dati::versato_singola(db, az, setup, &t.anno, &t.num, &t.codart, macchina.versa_solo_buoni).await?;
        let fatto = versato.get(&progr).copied().unwrap_or(0.0);
        // with InsParziale the quantity screen lists only what is still missing
        let max = if ins_parziale { (t.qta - fatto).max(0.0) } else { t.qta };
        if max <= 1e-9 {
            return Ok(EsitoLotto::Errore(format!(
                "{}-{}: già versata completamente sulla fase {progr} ({} su {}).",
                t.anno,
                t.num,
                dati::fmt_qta(fatto, setup.ndec),
                dati::fmt_qta(t.qta, setup.ndec)
            )));
        }
        Ok(EsitoLotto::Una(VoceLotto { t, progr, max, qta: max, esito: None }))
    }
    .await;
    res.unwrap_or_else(|e| EsitoLotto::Errore(format!("Verifica non riuscita: {e}")))
}

impl Main {
    /// From the BARCODE window: switch to the list mode.
    pub(super) fn apri_lotto(&mut self) {
        if matches!(self.dialoghi.last(), Some(Dialogo::Barcode { .. })) {
            self.dialoghi.pop();
        }
        self.scan_buf.clear();
        self.scan_ultimo = None;
        self.dialoghi.push(Dialogo::Lotto { voci: Vec::new(), errore: None, ricerca: false });
    }

    fn lotto_errore(&mut self, testo: String) {
        for d in self.dialoghi.iter_mut().rev() {
            if let Dialogo::Lotto { errore, ricerca, .. } = d {
                *errore = Some(testo);
                *ricerca = false;
                return;
            }
        }
    }

    fn lotto_cerca_su(&mut self) {
        for d in self.dialoghi.iter_mut().rev() {
            if let Dialogo::Lotto { errore, ricerca, .. } = d {
                *errore = None;
                *ricerca = true;
                return;
            }
        }
    }

    /// A scanned code in list mode: find it on the machine being controlled, then verify.
    pub(super) fn lotto_letto(&mut self, codice: String) {
        if !self.sola_lettura && !self.ponte.available() {
            self.lotto_errore("Il modulo di salvataggio non è disponibile: il versamento automatico non può partire.".into());
            return;
        }
        let Some(macchina) = self.macchina() else {
            self.lotto_errore(format!("«{codice}»: seleziona prima la macchina in MACCHINE."));
            return;
        };
        self.lotto_cerca_su();
        let (db, az, setup, ins) = (self.db.clone(), self.az.clone(), self.setup.clone(), self.post.ins_parziale);
        self.lotto_task = Some(self.db.spawn(async move {
            // only the machine being controlled, as the single flow
            let v = dati::cerca_singola(&db, &az, &codice, std::slice::from_ref(&macchina)).await?;
            Ok(match v.len() {
                0 => EsitoLotto::Errore(format!("«{codice}»: nessuna commessa aperta su {} corrisponde.", macchina.descr.trim())),
                1 => verifica(&db, &az, &setup, &macchina, v[0].clone(), ins).await,
                _ => EsitoLotto::Scelta(v),
            })
        }));
    }

    /// Several orders matched: the one picked from the list is verified.
    pub(super) fn lotto_scelto(&mut self, t: Trovata) {
        let Some(macchina) = self.macchina() else { return };
        self.lotto_cerca_su();
        let (db, az, setup, ins) = (self.db.clone(), self.az.clone(), self.setup.clone(), self.post.ins_parziale);
        self.lotto_task = Some(self.db.spawn(async move { Ok(verifica(&db, &az, &setup, &macchina, t, ins).await) }));
    }

    pub(super) fn poll_lotto(&mut self) {
        let Some(task) = &self.lotto_task else { return };
        let Some(res) = task.poll() else { return };
        self.lotto_task = None;
        match res {
            Err(e) => self.lotto_errore(format!("Ricerca non riuscita: {e}")),
            Ok(EsitoLotto::Errore(e)) => self.lotto_errore(e),
            Ok(EsitoLotto::Scelta(trovate)) => {
                for d in self.dialoghi.iter_mut().rev() {
                    if let Dialogo::Lotto { ricerca, .. } = d {
                        *ricerca = false;
                        break;
                    }
                }
                let codice = trovate.first().map(|t| t.num.clone()).unwrap_or_default();
                self.dialoghi.push(Dialogo::LottoScelta { codice, trovate });
            }
            Ok(EsitoLotto::Una(v)) => {
                let mut doppia = None;
                for d in self.dialoghi.iter_mut().rev() {
                    if let Dialogo::Lotto { voci, errore, ricerca } = d {
                        *ricerca = false;
                        if voci.iter().any(|x| x.t.anno == v.t.anno && x.t.num == v.t.num) {
                            doppia = Some(v.codice());
                        } else {
                            *errore = None;
                            voci.push(v);
                        }
                        break;
                    }
                }
                if let Some(c) = doppia {
                    self.lotto_errore(format!("{c} è già nella lista."));
                }
            }
        }
        // test option: next code of the list
        if self.lotto_task.is_none() && !self.prova_lotto.is_empty() && matches!(self.dialoghi.last(), Some(Dialogo::Lotto { .. })) {
            let c = self.prova_lotto.remove(0);
            self.lotto_letto(c);
        }
    }

    /// AVVIA: one run of the automatic flow per group order (R), in the order scanned.
    pub(super) fn avvia_lotto(&mut self, voci: Vec<VoceLotto>) {
        let mut gruppi: Vec<Vec<VoceLotto>> = Vec::new();
        for v in voci {
            match gruppi.iter_mut().find(|g| g[0].t.anno_r == v.t.anno_r && g[0].t.num_r == v.t.num_r) {
                Some(g) => g.push(v),
                None => gruppi.push(vec![v]),
            }
        }
        if self.sola_lettura {
            let n: usize = gruppi.iter().map(|g| g.len()).sum();
            self.messaggio(
                format!("Sola lettura: verrebbero versate {n} commesse in {} commesse R (una apertura e chiusura per ciascuna). Nessuna registrazione.", gruppi.len()),
                false,
            );
            return;
        }
        self.lotto_coda = gruppi;
        self.lotto_fatti.clear();
        self.lotto_prossimo();
    }

    /// Next group order of the batch, or the summary when none is left.
    pub(super) fn lotto_prossimo(&mut self) {
        if self.lotto_coda.is_empty() {
            if !self.lotto_fatti.is_empty() {
                self.lotto_riepilogo();
            }
            return;
        }
        let voci = self.lotto_coda.remove(0);
        let t = voci[0].t.clone();
        self.rapido = Some(Rapido {
            codice: format!("{}-{}", t.anno_r, t.num_r),
            t: None,
            mac: String::new(),
            mac_prima: self.mac.clone(),
            tab_prima: self.tab,
            avviata_commessa: false,
            avviata_fase: None,
            fase_prima: None,
            passo: Passo::Stato,
            cerca: None,
            stato: None,
            fasi: Vec::new(),
            registrato: false,
            lotto: Some(voci),
            problema: None,
        });
        self.scegli_rapido(t);
    }

    /// Quantity screen opened for the group: fill every order of the batch, then one save.
    pub(super) fn lotto_compila(&mut self) {
        let Some(r) = &mut self.rapido else { return };
        let Some(mut voci) = r.lotto.take() else { return };
        let mut scritte = 0;
        if let Some(v) = &mut self.vers {
            v.rapido = true;
            for voce in voci.iter_mut() {
                match v.righe.iter_mut().find(|x| x.anno.trim() == voce.t.anno.trim() && x.num.trim() == voce.t.num.trim()) {
                    Some(riga) => {
                        // never more than the quantity screen itself would propose, so the
                        // original's per-row checks at save (ESP_FormPezziRag::GetArticoliDaVersare
                        // 0x4e3f0: qty > da fare → 460/459 by BLOCCA_VERSAMENTO_MAGGIORE_COMMESSA,
                        // MG "N") are never hit; a previous-phase limit (471) fails the group and
                        // shows in the summary
                        let max = super::dialoghi::proposta(riga, v.versa_rimanenti);
                        let f = 10f64.powi(riga.ndec as i32);
                        let q = (voce.qta.min(max) * f).round() / f;
                        if q > 0.0 {
                            riga.qta = Some(q);
                            voce.esito = Some(Ok(q));
                            scritte += 1;
                        } else {
                            voce.esito = Some(Err("niente da versare in questa fase".into()));
                        }
                    }
                    None => voce.esito = Some(Err("non presente nel versamento della fase (già completa?)".into())),
                }
            }
        }
        if let Some(r) = &mut self.rapido {
            r.lotto = Some(voci);
        }
        if scritte > 0 {
            self.conferma_versamento();
        } else {
            self.ripristina_rapido(None);
        }
    }

    /// A group order finished (written, cancelled or failed): keep its orders' outcome.
    pub(super) fn lotto_gruppo_finito(&mut self, mut voci: Vec<VoceLotto>, registrato: bool, problema: Option<String>) {
        for v in voci.iter_mut() {
            let ok = matches!(v.esito, Some(Ok(_)));
            if ok && !registrato {
                v.esito = Some(Err(problema.clone().unwrap_or_else(|| "non registrata (annullata)".into())));
            } else if v.esito.is_none() {
                v.esito = Some(Err(problema.clone().unwrap_or_else(|| "non registrata".into())));
            }
        }
        self.lotto_fatti.extend(voci);
    }

    fn lotto_riepilogo(&mut self) {
        let fatti = std::mem::take(&mut self.lotto_fatti);
        let ok = fatti.iter().filter(|v| matches!(v.esito, Some(Ok(_)))).count();
        let mut testo = format!("Versamento multiplo: {ok} commesse registrate su {}.", fatti.len());
        for v in &fatti {
            if let Some(Err(e)) = &v.esito {
                testo.push_str(&format!("\n• {}: {e}", v.codice()));
            }
        }
        self.messaggio(testo, ok < fatti.len());
        self.ricarica_tutto();
    }
}

/// The list window: reader box, collected orders with quantity, AVVIA.
#[allow(clippy::too_many_arguments)]
pub(super) fn disegna_lotto(ui: &mut Ui, ctx: &egui::Context, voci: &mut Vec<VoceLotto>, errore: &Option<String>, ricerca: bool, lettura: &str, mac: &str, ndec: usize) -> Option<AzioneLotto> {
    let mut azione = None;
    let wd = 940.0;
    ui.set_width(wd);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
        let t = ctx.input(|i| i.time) as f32;
        if !ricerca {
            let f = (t * 1.6).fract();
            ui.painter().circle_stroke(r.center(), 22.0 + f * 6.0, Stroke::new(3.0, Color32::from_rgba_unmultiplied(ORANGE.r(), ORANGE.g(), ORANGE.b(), ((1.0 - f) * 140.0) as u8)));
        }
        ui.painter().circle_filled(r.center(), 22.0, lerp(ORANGE, Color32::WHITE, 0.85));
        w::icon(ui.painter(), r.shrink(12.0), Icon::Barcode, ORANGE_DARK);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Versamento di più commesse").font(bold(24.0)).color(TEXT));
            let sotto = format!("Spara le commesse una dopo l'altra: vengono solo verificate su {mac}. Il versamento parte con AVVIA.");
            ui.label(egui::RichText::new(sotto).size(16.0).color(MUTED));
        });
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(30));
    ui.add_space(10.0);
    // reader / status line
    let (lr, _) = ui.allocate_exact_size(vec2(wd, 52.0), Sense::hover());
    let p = ui.painter();
    let (bg, testo, col) = match (ricerca, errore, lettura.is_empty()) {
        (true, _, _) => (CARD_SOFT, "Verifica…".to_string(), MUTED),
        (false, Some(e), true) => (lerp(DANGER, Color32::WHITE, 0.88), e.clone(), DANGER),
        (false, _, false) => (CARD_SOFT, lettura.to_string(), PRIMARY_DARK),
        (false, None, true) => (CARD_SOFT, "in attesa del codice".to_string(), FAINT),
    };
    p.rect_filled(lr, 10.0, bg);
    p.rect_stroke(lr, 10.0, Stroke::new(1.0, BORDER_HI), StrokeKind::Inside);
    p.text(lr.center(), Align2::CENTER_CENTER, w::elide(p, &testo, bold(18.0), wd - 30.0), bold(18.0), col);
    if ricerca {
        ui.put(Rect::from_center_size(lr.right_center() - vec2(30.0, 0.0), vec2(24.0, 24.0)), egui::Spinner::new().size(22.0).color(PRIMARY));
    }
    ui.add_space(10.0);

    // collected orders
    let gruppi = {
        let mut g: Vec<(String, String)> = voci.iter().map(|v| (v.t.anno_r.clone(), v.t.num_r.clone())).collect();
        g.sort();
        g.dedup();
        g.len()
    };
    let head = if voci.is_empty() {
        "Nessuna commessa in lista".to_string()
    } else {
        let n = voci.len();
        format!(
            "{n} {} · {gruppi} {} → la macchina viene aperta e chiusa {}",
            if n == 1 { "commessa" } else { "commesse" },
            if gruppi == 1 { "commessa R" } else { "commesse R" },
            if gruppi == 1 { "una volta".to_string() } else { format!("{gruppi} volte") }
        )
    };
    ui.label(egui::RichText::new(head).font(bold(17.0)).color(TEXT));
    ui.add_space(4.0);
    let mut togli = None;
    egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).scroll_source(egui::scroll_area::ScrollSource::ALL).show(ui, |ui| {
        for (i, v) in voci.iter_mut().enumerate() {
            egui::Frame::new().fill(CARD_SOFT).stroke(Stroke::new(1.0, BORDER)).corner_radius(10).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(wd - 40.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (tr, _) = ui.allocate_exact_size(vec2(430.0, 52.0), Sense::hover());
                    let p = ui.painter();
                    p.text(tr.left_top() + vec2(0.0, 2.0), Align2::LEFT_TOP, v.codice(), bold(21.0), TEXT);
                    let sotto = format!("R {}-{} · fase {} · {}", v.t.anno_r, v.t.num_r, v.progr, v.t.codart);
                    p.text(tr.left_top() + vec2(0.0, 30.0), Align2::LEFT_TOP, w::elide(p, &sotto, reg(15.0), 425.0), reg(15.0), MUTED);
                    if w::tile(ui, vec2(60.0, 52.0), None, "−", Kind::Normal, v.qta > 1.0).clicked() {
                        v.qta = (v.qta - 1.0).max(1.0);
                    }
                    let (qr, qresp) = ui.allocate_exact_size(vec2(170.0, 52.0), Sense::click());
                    ui.painter().rect_filled(qr, 10.0, SEL_FILL);
                    ui.painter().rect_stroke(qr, 10.0, Stroke::new(2.0, SEL_BORDER), StrokeKind::Inside);
                    ui.painter().text(qr.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, dati::fmt_qta(v.qta, ndec), bold(22.0), PRIMARY_DARK);
                    ui.painter().text(qr.center() + vec2(0.0, 15.0), Align2::CENTER_CENTER, format!("max {} {}", dati::fmt_qta(v.max, ndec), v.t.um), reg(12.0), MUTED);
                    if qresp.clicked() {
                        azione = Some(AzioneLotto::Tastiera(i));
                    }
                    if w::tile(ui, vec2(60.0, 52.0), None, "+", Kind::Normal, v.qta < v.max).clicked() {
                        v.qta = (v.qta + 1.0).min(v.max);
                    }
                    if w::tile(ui, vec2(60.0, 52.0), Some(Icon::Close), "", Kind::Normal, true).clicked() {
                        togli = Some(i);
                    }
                });
            });
            ui.add_space(6.0);
        }
    });
    if let Some(i) = togli {
        voci.remove(i);
    }
    ui.add_space(12.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let n = voci.len();
        if w::tile(ui, vec2(290.0, 68.0), Some(Icon::Play), &format!("AVVIA VERSAMENTO ({n})"), Kind::Good, n > 0 && !ricerca).clicked() {
            azione = Some(AzioneLotto::Avvia);
        }
        if w::tile(ui, vec2(170.0, 68.0), Some(Icon::Close), "ANNULLA", Kind::Normal, true).clicked() {
            azione = Some(AzioneLotto::Annulla);
        }
        if w::tile(ui, vec2(250.0, 68.0), Some(Icon::Keyboard), "INSERISCI A MANO", Kind::Normal, !ricerca).clicked() {
            azione = Some(AzioneLotto::Mano);
        }
    });
    azione
}

/// Quantity typed for a list row: 0 < q ≤ max, rounded to the order's decimals.
pub(super) fn qta_lotto(voci: &mut [VoceLotto], i: usize, testo: &str, ndec: usize) {
    if let (Some(v), Ok(q)) = (voci.get_mut(i), testo.trim().replace(',', ".").parse::<f64>()) {
        let f = 10f64.powi(ndec as i32);
        v.qta = ((q.clamp(0.0, v.max)) * f).round() / f;
        if v.qta <= 0.0 {
            v.qta = v.max.min(1.0);
        }
    }
}
