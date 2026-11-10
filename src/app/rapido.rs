//! Automatic flow from one barcode scan (new, built on the original's actions):
//! find the single order → start its group order on the machine (IniziaCommessa_Click)
//! → start / use its production phase (IniziaFase_Click / bUtilizzaFase_Click) →
//! quantity screen with the scanned order matched and the quantity pre-filled → one
//! confirmation writes it (ESP_FormPezziRag) → undo only what the flow started
//! (ChiudiSospendiFase 'R', then bSospendiCommessa_Click — allowed because
//! GestoreFasi.SospendiFase clears PD_DTEINI) → back to the starting screen.

use std::time::{Duration, Instant};

use eframe::egui;

use super::{Dialogo, Dopo, Main, Tab, corrispondenze};
use crate::dati::{self, Ambiente, Commessa, Fase, FiltroCommesse, StatoFase, Trovata};
use crate::db::Task;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Passo {
    Cerca,
    Scelta,
    Stato,
    Commessa,
    Utilizza,
    Fase,
    Versa,
    RipFase,
    RipUtilizza,
    RipCommessa,
}

impl Passo {
    fn ripristino(self) -> bool {
        matches!(self, Passo::RipFase | Passo::RipUtilizza | Passo::RipCommessa)
    }
}

/// Machine state, the group's card on that machine, its phases there, and the pieces
/// already recorded for the scanned single order per phase.
type Stato = (Ambiente, Option<Commessa>, Vec<Fase>, std::collections::HashMap<i64, f64>);

pub(super) struct Rapido {
    pub codice: String,
    pub t: Option<Trovata>,
    pub mac: String,
    pub mac_prima: Option<String>,
    pub tab_prima: Tab,
    pub avviata_commessa: bool,
    pub avviata_fase: Option<i64>,
    pub fase_prima: Option<i64>,
    pub passo: Passo,
    pub cerca: Option<Task<Vec<Trovata>>>,
    pub stato: Option<Task<Stato>>,
    pub fasi: Vec<Fase>,
    pub registrato: bool,
    /// Batch (PIÙ COMMESSE): the orders of this group order, written in one save.
    pub lotto: Option<Vec<super::lotto::VoceLotto>>,
    /// Batch: why the group did not go through (kept for the final summary).
    pub problema: Option<String>,
}

impl Rapido {
    /// One line for the status bar.
    pub fn descrizione(&self) -> String {
        let c = match (&self.lotto, &self.t) {
            (Some(v), Some(t)) => format!("MULTIPLO R {}-{} ({} commesse)", t.anno_r, t.num_r, v.len()),
            _ => self.t.as_ref().map(|t| format!("{}-{}", t.anno, t.num)).unwrap_or_else(|| self.codice.clone()),
        };
        let fase = match self.passo {
            Passo::Cerca => "ricerca commessa…",
            Passo::Scelta => "scegli la commessa",
            Passo::Stato => "lettura macchina…",
            Passo::Commessa => "avvio commessa…",
            Passo::Utilizza => "selezione fase…",
            Passo::Fase => "avvio fase…",
            Passo::Versa => "versamento: conferma la quantità",
            Passo::RipFase => "sospensione fase…",
            Passo::RipUtilizza => "ripristino fase…",
            Passo::RipCommessa => "rilascio commessa…",
        };
        format!("BARCODE {c} · {} · {fase}", self.mac)
    }
}

const PAUSA_SCANNER: Duration = Duration::from_millis(300);

/// The production phase the flow would use: the running one, else the first not finished.
pub(super) fn fase_bersaglio(fasi: &[Fase]) -> Option<i64> {
    let produzione = fasi.iter().filter(|f| f.produzione());
    let mut in_corso = produzione.clone().filter(|f| f.stato == StatoFase::InCorso);
    in_corso
        .next()
        .or_else(|| produzione.clone().find(|f| f.stato != StatoFase::Terminata))
        .map(|f| f.progr)
}

impl Main {
    /// BARCODE button: open the listening window (main screens and quantity screen).
    pub(super) fn apri_barcode(&mut self) {
        if self.bloccato() || self.rapido.is_some() || matches!(self.dialoghi.last(), Some(Dialogo::Barcode { .. })) {
            return;
        }
        self.scan_buf.clear();
        self.scan_ultimo = None;
        self.dialoghi.push(Dialogo::Barcode { errore: None, ricerca: false, info: false });
    }

    /// INFO COMMESSA: the same listening window, but the scan only looks the code up
    /// (any machine, open or closed orders) and writes nothing.
    pub(super) fn apri_info(&mut self) {
        if self.bloccato() || matches!(self.dialoghi.last(), Some(Dialogo::Barcode { .. })) {
            return;
        }
        self.scan_buf.clear();
        self.scan_ultimo = None;
        self.dialoghi.push(Dialogo::Barcode { errore: None, ricerca: false, info: true });
    }

    fn cerca_info(&mut self, codice: String) {
        if let Some(Dialogo::Barcode { errore, ricerca, .. }) = self.dialoghi.last_mut() {
            *errore = None;
            *ricerca = true;
        }
        // only the machines configured for this workstation (Esprit.ini MACCHINE)
        let postazione = self.macchine.value.clone().unwrap_or_default();
        let (db, az, setup, c2) = (self.db.clone(), self.az.clone(), self.setup.clone(), codice.clone());
        self.info = Some((codice, self.db.spawn(async move { dati::info_codice(&db, &az, &setup, &c2, &postazione).await })));
    }

    pub(super) fn poll_info(&mut self) {
        let Some((codice, t)) = &self.info else { return };
        let Some(res) = t.poll() else { return };
        let codice = codice.clone();
        self.info = None;
        match res {
            Err(e) => {
                self.barcode_errore(format!("Ricerca non riuscita: {e}"));
            }
            Ok(v) if v.is_empty() => {
                self.barcode_errore(format!("«{codice}»: nessuna commessa con questo numero sulle macchine di questa postazione."));
            }
            Ok(trovate) => {
                self.chiudi_barcode();
                self.dialoghi.push(Dialogo::InfoCommessa { codice, trovate });
            }
        }
    }

    /// Show a problem in the barcode window and listen again; false if it is not open.
    fn barcode_errore(&mut self, testo: String) -> bool {
        match self.dialoghi.last_mut() {
            Some(Dialogo::Barcode { errore, ricerca, .. }) => {
                *errore = Some(testo);
                *ricerca = false;
                true
            }
            _ => false,
        }
    }

    fn chiudi_barcode(&mut self) {
        if matches!(self.dialoghi.last(), Some(Dialogo::Barcode { .. })) {
            self.dialoghi.pop();
        }
        self.scan_buf.clear();
    }

    /// A code from the barcode window (reader or typed by hand). Quantity screen: match
    /// its rows; main screens: start the automatic flow on the machine being controlled.
    /// The window closes on success and keeps listening otherwise.
    pub(super) fn barcode_letto(&mut self, codice: String) {
        let codice = codice.trim().to_string();
        if matches!(self.dialoghi.last(), Some(Dialogo::Barcode { info: true, .. })) {
            self.cerca_info(codice);
            return;
        }
        // PIÙ COMMESSE: only verify and add to the list (lotto.rs)
        if matches!(self.dialoghi.last(), Some(Dialogo::Lotto { .. })) {
            self.lotto_letto(codice);
            return;
        }
        if let Some(v) = &mut self.vers {
            v.ultimo_codice = codice.clone();
            if corrispondenze(&v.righe, &codice).is_empty() {
                let testo = if v.righe.is_empty() {
                    format!("«{codice}»: niente da versare in questa fase, le quantità risultano già tutte registrate.")
                } else {
                    format!("«{codice}»: nessuna commessa ancora da versare in questa fase corrisponde (potrebbe essere già versata completamente).")
                };
                self.barcode_errore(testo);
            } else {
                self.chiudi_barcode();
                self.codice_letto(codice);
            }
            return;
        }
        if !self.sola_lettura && !self.ponte.available() {
            self.barcode_errore("Il modulo di salvataggio non è disponibile: il versamento automatico non può partire.".into());
            return;
        }
        if self.mac.is_none() {
            self.barcode_errore(format!("«{codice}»: seleziona prima la macchina in MACCHINE."));
            return;
        }
        if let Some(Dialogo::Barcode { errore, ricerca, .. }) = self.dialoghi.last_mut() {
            *errore = None;
            *ricerca = true;
        }
        self.avvia_rapido(codice);
    }

    /// Barcode reader (keyboard wedge): listens only while the barcode window is open
    /// and not searching; Enter/Tab ends a code, or a pause after 5+ characters.
    pub(super) fn ascolta_barcode(&mut self, ctx: &egui::Context) {
        let attivo = matches!(self.dialoghi.last(), Some(Dialogo::Barcode { ricerca: false, .. } | Dialogo::Lotto { ricerca: false, .. }))
            && self.azione.is_none()
            && self.rapido.is_none()
            && self.info.is_none()
            && self.lotto_task.is_none()
            && !self.splash;
        if !attivo {
            self.scan_buf.clear();
            return;
        }
        let mut pronto: Option<String> = None;
        ctx.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Text(t) => {
                        for ch in t.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '/') {
                            self.scan_buf.push(ch);
                        }
                        self.scan_ultimo = Some(Instant::now());
                    }
                    egui::Event::Key { key: egui::Key::Enter | egui::Key::Tab, pressed: true, .. } => {
                        if !self.scan_buf.trim().is_empty() {
                            pronto = Some(std::mem::take(&mut self.scan_buf));
                        }
                    }
                    _ => {}
                }
            }
        });
        if pronto.is_none() && !self.scan_buf.is_empty() {
            if self.scan_ultimo.map(|t| t.elapsed() >= PAUSA_SCANNER).unwrap_or(true) {
                let b = std::mem::take(&mut self.scan_buf);
                if b.trim().len() >= 5 {
                    pronto = Some(b);
                }
            } else {
                ctx.request_repaint_after(PAUSA_SCANNER);
            }
        }
        if let Some(c) = pronto {
            self.barcode_letto(c);
        }
    }

    pub(super) fn avvia_rapido(&mut self, codice: String) {
        if !self.sola_lettura && !self.ponte.available() {
            self.messaggio("Il modulo di salvataggio non è disponibile: il versamento automatico non può partire.", true);
            return;
        }
        // Only the machine being controlled (its own phases or its family's), not the others
        let Some(macchina) = self.macchina() else {
            self.messaggio(format!("Codice «{codice}» letto: seleziona prima la macchina in MACCHINE."), true);
            return;
        };
        let solo_questa = vec![macchina];
        let (db, az, c2) = (self.db.clone(), self.az.clone(), codice.clone());
        let task = self.db.spawn(async move { dati::cerca_singola(&db, &az, &c2, &solo_questa).await });
        self.rapido = Some(Rapido {
            codice,
            t: None,
            mac: String::new(),
            mac_prima: self.mac.clone(),
            tab_prima: self.tab,
            avviata_commessa: false,
            avviata_fase: None,
            fase_prima: None,
            passo: Passo::Cerca,
            cerca: Some(task),
            stato: None,
            fasi: Vec::new(),
            registrato: false,
            lotto: None,
            problema: None,
        });
    }

    pub(super) fn poll_rapido(&mut self) {
        let Some(r) = &mut self.rapido else { return };
        if let Some(t) = &r.cerca {
            if let Some(res) = t.poll() {
                r.cerca = None;
                // Nothing touched yet: a failed search stays in the barcode window, listening again
                let problema = match &res {
                    Err(e) => Some(format!("Ricerca della commessa non riuscita: {e}")),
                    Ok(v) if v.is_empty() => {
                        let codice = r.codice.clone();
                        let m = self.macchina().map(|m| if m.descr.trim().is_empty() { m.cod } else { m.descr }).unwrap_or_default();
                        Some(format!("«{codice}»: nessuna commessa aperta sulla macchina {m} corrisponde."))
                    }
                    Ok(_) => None,
                };
                if let Some(p) = problema {
                    self.rapido = None;
                    if !self.barcode_errore(p.clone()) {
                        self.messaggio(p, true);
                    }
                    return;
                }
                self.chiudi_barcode();
                let r = self.rapido.as_mut().unwrap();
                match res {
                    Ok(v) if v.len() == 1 => self.scegli_rapido(v[0].clone()),
                    Ok(v) => {
                        r.passo = Passo::Scelta;
                        let codice = r.codice.clone();
                        self.dialoghi.push(Dialogo::RapidoScelta { codice, trovate: v });
                    }
                    Err(_) => {}
                }
                return;
            }
        }
        if let Some(t) = &r.stato {
            if let Some(res) = t.poll() {
                r.stato = None;
                match res {
                    Err(e) => self.fine_rapido(Some((format!("Lettura della macchina non riuscita: {e}"), true))),
                    Ok(s) => self.decidi_rapido(s),
                }
            }
        }
    }

    /// The machine: the one on screen if it works this order, else the first of the workstation.
    pub(super) fn scegli_rapido(&mut self, t: Trovata) {
        let Some(r) = &mut self.rapido else { return };
        let mac = match &self.mac {
            Some(m) if t.macchine.contains(m) => m.clone(),
            _ => t.macchine[0].clone(),
        };
        let Some(macchina) = self.macchine.value.as_ref().and_then(|v| v.iter().find(|m| m.cod == mac).cloned()) else {
            self.fine_rapido(Some(("Macchina non trovata".into(), true)));
            return;
        };
        r.mac = mac.clone();
        r.passo = Passo::Stato;
        r.t = Some(t.clone());
        let (db, az, setup, ins, max_note) = (self.db.clone(), self.az.clone(), self.setup.clone(), self.post.ins_parziale, self.post.max_len_note);
        r.stato = Some(self.db.spawn(async move {
            let amb = dati::ambiente(&db, &az, &mac).await?;
            let filtro = FiltroCommesse { anno: t.anno.parse().ok(), num: t.num.parse().ok() };
            let comm = dati::commesse(&db, &az, &setup, &macchina, &filtro, max_note).await?.into_iter().find(|c| c.anno == t.anno_r && c.num == t.num_r);
            // phases of the group order on this machine
            let mut amb_r = amb.clone();
            if !(amb.anno == t.anno_r && amb.num == t.num_r) {
                amb_r.progr = String::new();
            }
            amb_r.anno = t.anno_r.clone();
            amb_r.num = t.num_r.clone();
            amb_r.tipo = "R".into();
            let fasi = dati::fasi(&db, &az, &setup, &amb_r, &mac, ins).await?;
            let versato = dati::versato_singola(&db, &az, &setup, &t.anno, &t.num, &t.codart, macchina.versa_solo_buoni).await?;
            Ok((amb, comm, fasi, versato))
        }));
    }

    fn decidi_rapido(&mut self, (amb, comm, fasi, versato): Stato) {
        let Some(r) = &mut self.rapido else { return };
        let t = r.t.clone().expect("order");
        r.fasi = fasi;
        // Before touching the machine: is anything left to record for the scanned order on
        // the phase the flow would use? With InsParziale the original subtracts what was
        // already recorded (Versamento.GetArticoliDaProdurre) and would list nothing.
        // (a batch was checked order by order when scanned)
        if let Some(progr) = fase_bersaglio(&r.fasi).filter(|_| r.lotto.is_none()) {
            let fatto = versato.get(&progr).copied().unwrap_or(0.0);
            if self.post.ins_parziale && t.qta > 0.0 && fatto >= t.qta - 1e-9 {
                let mac = r.mac.clone();
                let nd = self.setup.ndec;
                self.fine_rapido(Some((
                    format!(
                        "La commessa {}-{} è già versata completamente sulla fase {progr} di {mac} ({} su {} {}). Non c'è nulla da registrare: la macchina non è stata toccata. Se il lavoro è finito, chiudi la fase con TERMINA FASE.",
                        t.anno, t.num, dati::fmt_qta(fatto, nd), dati::fmt_qta(t.qta, nd), t.um
                    ),
                    true,
                )));
                return;
            }
        }
        if self.sola_lettura {
            // Read-only: say what would happen and stop before any write
            let mac = r.mac.clone();
            let azione = if amb.cod_dip.trim().is_empty() {
                "manca l'operatore sulla macchina".to_string()
            } else if amb.anno.is_empty() {
                "avvierebbe la commessa, la fase e il versamento".to_string()
            } else if amb.anno == t.anno_r && amb.num == t.num_r {
                "la commessa è già sulla macchina: aprirebbe il versamento".to_string()
            } else {
                format!("la macchina sta lavorando {}", amb.commessa_label())
            };
            self.fine_rapido(None);
            self.messaggio(
                format!("Trovata {}-{} (gruppo R {}-{}) su {mac}: {azione}.\nSola lettura: nessuna registrazione.", t.anno, t.num, t.anno_r, t.num_r),
                false,
            );
            return;
        }
        if amb.cod_dip.trim().is_empty() {
            let m = r.mac.clone();
            self.fine_rapido(Some((format!("Nessun operatore sulla macchina {m}: in OPERATORI inizia il turno e premi PRENDI MACCHINA."), true)));
            return;
        }
        if !amb.anno.is_empty() && !(amb.anno == t.anno_r && amb.num == t.num_r) {
            let m = r.mac.clone();
            let c = amb.commessa_label();
            self.fine_rapido(Some((format!("La macchina {m} sta lavorando la commessa {c}: terminala o sospendila prima di versare un'altra commessa."), true)));
            return;
        }
        // the screen follows the machine of the flow
        let mac = r.mac.clone();
        if self.mac.as_deref() != Some(mac.as_str()) {
            self.seleziona_macchina(mac.clone());
        }
        let r = self.rapido.as_mut().unwrap();
        if amb.anno.is_empty() {
            r.passo = Passo::Commessa;
            r.avviata_commessa = true;
            let sem = comm.map(|c| c.semaforo.codice().to_string()).unwrap_or_else(|| "R".into());
            self.esegui(
                "commessa_inizia",
                vec![("mac", mac), ("anno", t.anno_r), ("num", t.num_r), ("tipo", "R".into()), ("semaforo", sem)],
                Dopo::Rapido,
            );
        } else {
            let usata = amb.progr.parse::<i64>().ok();
            self.fase_rapido(usata);
        }
    }

    /// Production phase: the one running (made the one in use if needed), else the
    /// first not finished, started now.
    fn fase_rapido(&mut self, usata: Option<i64>) {
        let Some(r) = &mut self.rapido else { return };
        let mac = r.mac.clone();
        let produzione: Vec<&Fase> = r.fasi.iter().filter(|f| f.produzione()).collect();
        if let Some(f) = produzione.iter().find(|f| f.stato == StatoFase::InCorso) {
            if usata == Some(f.progr) {
                self.versa_rapido();
            } else {
                r.fase_prima = usata;
                r.passo = Passo::Utilizza;
                let p = f.progr;
                self.esegui("fase_utilizza", vec![("mac", mac), ("progr", p.to_string())], Dopo::Rapido);
            }
            return;
        }
        let Some(f) = produzione.iter().find(|f| f.stato != StatoFase::Terminata) else {
            self.ripristina_rapido(Some("Nessuna fase di produzione aperta per questa commessa su questa macchina.".into()));
            return;
        };
        r.passo = Passo::Fase;
        r.avviata_fase = Some(f.progr);
        let (p, sem) = (f.progr, f.semaforo.codice().to_string());
        self.esegui("fase_inizia", vec![("mac", mac), ("progr", p.to_string()), ("semaforo", sem)], Dopo::Rapido);
    }

    fn versa_rapido(&mut self) {
        if let Some(r) = &mut self.rapido {
            r.passo = Passo::Versa;
        }
        self.versa_pezzi();
    }

    /// After the quantity screen opened during the flow: match the scanned order.
    pub(super) fn vers_aperto_rapido(&mut self) {
        let Some(r) = &self.rapido else { return };
        if r.passo != Passo::Versa {
            return;
        }
        if r.lotto.is_some() {
            self.lotto_compila();
            return;
        }
        let codice = r.t.as_ref().map(|t| format!("{}-{}", t.anno, t.num)).unwrap_or_default();
        if let Some(v) = &mut self.vers {
            v.rapido = true;
        }
        self.codice_letto(codice);
    }

    /// Answer of a save during the flow.
    pub(super) fn rapido_ok(&mut self, fatto: bool) {
        let Some(r) = &mut self.rapido else { return };
        let passo = r.passo;
        if !fatto {
            // a question of the original was answered "no"
            match passo {
                Passo::Commessa => r.avviata_commessa = false,
                Passo::Fase => r.avviata_fase = None,
                Passo::Utilizza => r.fase_prima = None,
                _ => {}
            }
            if passo.ripristino() {
                self.fine_rapido(Some(("Ripristino interrotto: controlla la macchina.".into(), true)));
            } else {
                self.ripristina_rapido(None);
            }
            return;
        }
        match passo {
            Passo::Commessa => self.fase_rapido(None),
            Passo::Utilizza | Passo::Fase => self.versa_rapido(),
            Passo::RipFase | Passo::RipUtilizza | Passo::RipCommessa => self.ripristina_rapido(None),
            _ => {}
        }
    }

    pub(super) fn rapido_errore(&mut self, e: String) {
        let Some(r) = &self.rapido else { return };
        if r.passo.ripristino() {
            self.fine_rapido(Some((format!("Ripristino non completato: {e}"), true)));
        } else {
            self.ripristina_rapido(Some(e));
        }
    }

    /// Quantity written (or cancelled): undo what the flow started, one step at a time.
    pub(super) fn ripristina_rapido(&mut self, problema: Option<String>) {
        if let Some(p) = problema {
            match &mut self.rapido {
                // batch: shown in the final summary, the next group goes on
                Some(r) if r.lotto.is_some() => r.problema = Some(p),
                _ => self.messaggio(p, true),
            }
        }
        self.vers = None;
        let Some(r) = &mut self.rapido else { return };
        let mac = r.mac.clone();
        if let Some(p) = r.avviata_fase.take() {
            r.passo = Passo::RipFase;
            self.esegui("fase_chiudi", vec![("mac", mac), ("progr", p.to_string()), ("funz", "R".into())], Dopo::Rapido);
        } else if let Some(p) = r.fase_prima.take() {
            r.passo = Passo::RipUtilizza;
            self.esegui("fase_utilizza", vec![("mac", mac), ("progr", p.to_string())], Dopo::Rapido);
        } else if std::mem::take(&mut r.avviata_commessa) {
            r.passo = Passo::RipCommessa;
            self.esegui("commessa_sospendi", vec![("mac", mac)], Dopo::Rapido);
        } else {
            let msg = if r.registrato {
                let c = r.t.as_ref().map(|t| format!("{}-{}", t.anno, t.num)).unwrap_or_default();
                Some((format!("Versamento della commessa {c} registrato."), false))
            } else {
                None
            };
            self.fine_rapido(msg);
        }
    }

    /// Back to the starting screen and machine.
    pub(super) fn fine_rapido(&mut self, msg: Option<(String, bool)>) {
        let Some(mut r) = self.rapido.take() else { return };
        self.vers = None;
        // batch: record how this group went, then the next group (or the summary)
        if let Some(voci) = r.lotto.take() {
            let problema = msg.filter(|(_, e)| *e).map(|(t, _)| t).or(r.problema.take());
            self.lotto_gruppo_finito(voci, r.registrato, problema);
            if let Some(m) = r.mac_prima.filter(|m| self.mac.as_deref() != Some(m.as_str())) {
                self.seleziona_macchina(m);
            }
            self.tab = r.tab_prima;
            self.lotto_prossimo();
            return;
        }
        if let Some(m) = r.mac_prima {
            if self.mac.as_deref() != Some(m.as_str()) {
                self.seleziona_macchina(m);
            } else {
                self.ricarica_tutto();
            }
        }
        self.tab = r.tab_prima;
        if let Some((t, errore)) = msg {
            if errore {
                self.messaggio(t, true);
            } else {
                self.avvisa(t, false);
            }
        }
    }
}
