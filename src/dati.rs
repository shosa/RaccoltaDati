//! Everything the screens read, rebuilt from the original's queries
//! (ESP_FormMain.Refresh*, CommessaRD, RD.Logic.Funzioni). Where the original ran
//! one query per card (and several per traffic light), these run as a few
//! combined queries; the rules are the same (see MECHANICS.md).

use std::collections::HashMap;

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime};

use crate::db::{Cell, Db, Param, Sql, Table};

// ---------------------------------------------------------------------------
// Settings (RD.Logic.Settings / impostazioni, read with spCN_SETUPGetSingleValue)

#[derive(Clone, Debug)]
pub struct Setup {
    pub ndec: usize,
    pub ord_comme: String,
    pub days_show: i64,
    pub blocca_rosso: bool,
    pub max_commesse: usize,
    pub vf: String,
    pub vs: String,
    pub ore: String,
    pub ordine_fase: bool,
    pub show_dt_consegna: bool,
    pub nascondi_fasi_terminate: bool,
    pub prelievo: bool,
    pub manometro_fasi_intermedie: bool,
    /// LABEL_CONTROLLO_MACCHINA (Settings::.ctor, default ""): caption of the take-machine
    /// button when set (ESP_FormMain::.ctor 0x1a054).
    pub label_controllo: String,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            ndec: 0,
            ord_comme: "P".into(),
            days_show: 999,
            blocca_rosso: true,
            max_commesse: 0,
            vf: "VF".into(),
            vs: "VS".into(),
            ore: "ORE".into(),
            ordine_fase: true,
            show_dt_consegna: false,
            nascondi_fasi_terminate: false,
            prelievo: true,
            manometro_fasi_intermedie: false,
            label_controllo: String::new(),
        }
    }
}

async fn setup_value(db: &Db, az: &str, name: &str) -> Option<String> {
    let mut s = Sql::new("");
    let a = s.arg(az);
    let n = s.arg(name);
    s.text = format!("EXEC spCN_SETUPGetSingleValue @IdAz = {a}, @NomeS = {n}, @IdGruppo = 1, @IdUser = 1");
    let t = db.query(s).await.ok()?;
    let row = t.rows.first()?;
    let v = t.get(row, "SE_VALORE");
    if v.is_null() { None } else { Some(v.text()) }
}

pub async fn load_setup(db: &Db, az: &str) -> Setup {
    let d = Setup::default();
    let names = [
        "NDECQTA",
        "ORD_COMME_MES",
        "DAYS_SHOW_COMMESSE",
        "BLOCCA_SE_SEMAFORO_ROSSO",
        "MAX_COMMESSE_CARICATE_MES",
        "CAUSALE_VERS_FASE_ULTIMA",
        "CAUSALE_VERS_FASE_INTERM",
        "OPERAZIONE_LAVORO_SU_FASE",
        "AVANZORDINEFASI",
        "MES_COMMESSE_SHOW_DT_CONSEGNA",
        "NASCONDI_FASI_TERMINATE",
        "PRELIEVO_MATERIALI",
        "MANOMETRO_CONTA_PEZZI_FASI_INTERMEDIE",
        "LABEL_CONTROLLO_MACCHINA",
    ];
    let futs = names.iter().map(|n| setup_value(db, az, n));
    let vals: Vec<Option<String>> = futures_util::future::join_all(futs).await;
    let v = |i: usize| vals[i].clone();
    Setup {
        ndec: v(0).and_then(|x| x.trim().parse().ok()).unwrap_or(d.ndec),
        ord_comme: v(1).unwrap_or(d.ord_comme),
        days_show: v(2).and_then(|x| x.trim().parse().ok()).unwrap_or(d.days_show),
        blocca_rosso: v(3).map(|x| x == "Y").unwrap_or(d.blocca_rosso),
        max_commesse: v(4).and_then(|x| x.trim().parse().ok()).unwrap_or(d.max_commesse),
        vf: v(5).unwrap_or(d.vf),
        vs: v(6).unwrap_or(d.vs),
        ore: v(7).unwrap_or(d.ore),
        ordine_fase: v(8).map(|x| x == "OF").unwrap_or(true),
        show_dt_consegna: v(9).map(|x| x == "Y").unwrap_or(false),
        nascondi_fasi_terminate: v(10).map(|x| x == "Y").unwrap_or(false),
        prelievo: v(11).map(|x| x == "Y").unwrap_or(true),
        manometro_fasi_intermedie: v(12).map(|x| x == "Y").unwrap_or(false),
        label_controllo: v(13).map(|x| x.trim().to_string()).unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Formatting

/// Funz.NullToEmptyAndTruncate(qta, nDec): "0.000" pattern, Italian comma, no grouping.
pub fn fmt_qta(v: f64, ndec: usize) -> String {
    // an empty f64 sum is -0.0: show it as 0
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{:.*}", ndec, v).replace('.', ",");
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == ',') { s[1..].to_string() } else { s }
}

pub fn fmt_durata(sec: f64) -> String {
    if !sec.is_finite() || sec < 0.0 {
        return "—".into();
    }
    let s = sec.round() as i64;
    format!("{}:{:02}", s / 3600, (s % 3600) / 60)
}

fn cell_i(c: &Cell) -> i64 {
    c.i64().unwrap_or(0)
}

fn cell_f(c: &Cell) -> f64 {
    c.f64().unwrap_or(0.0)
}

fn cell_date(c: &Cell) -> Option<NaiveDate> {
    match c {
        Cell::Date(d) => Some(*d),
        Cell::DateTime(d) => Some(d.date()),
        _ => None,
    }
}

fn cell_time(c: &Cell) -> Option<NaiveTime> {
    match c {
        Cell::Time(t) => Some(*t),
        Cell::DateTime(d) => Some(d.time()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Machines (RefreshMacchine + Macchina control)

#[derive(Clone, Debug, Default)]
pub struct Macchina {
    pub cod: String,
    pub descr: String,
    pub famiglia: String,
    pub operatore: String,
    pub operatore_nome: String,
    pub anno: String,
    pub num: String,
    pub tipo: String,
    pub progr: String,
    pub fase_descr: String,
    pub versa_solo_buoni: bool,
}

impl Macchina {
    pub fn commessa_label(&self) -> String {
        if self.anno.is_empty() {
            return String::new();
        }
        let r = if self.tipo == "R" { "R " } else { "" };
        format!("{r}{}-{}", self.anno, self.num)
    }
}

pub async fn macchine(db: &Db, az: &str, codici: &[String]) -> Result<Vec<Macchina>, String> {
    let mut s = Sql::new(
        "SELECT C.CODICE, C.DESCR, C.CE_FAMIGLIA, C.CE_VERSA_SOLO_BUONI, R.RD_CODDIP, U.NOME, R.RD_ANNOCOM, R.RD_NUMCOM, R.RD_TIPOCOM, R.RD_PROGRFASE, F.PD_DESFAS
         FROM CELPRO C
         LEFT JOIN RD_MACCHINE R ON R.RD_CODMAC = C.CODICE AND R.RD_IDAZIENDA = C.CE_IDAZIENDA
         LEFT JOIN Users U ON U.USERNAME = R.RD_CODDIP AND U.IDAZIENDA = R.RD_IDAZIENDA
         LEFT JOIN PD_FASI F ON F.PD_IDAZIENDA = R.RD_IDAZIENDA AND F.PD_ANNOCOM = R.RD_ANNOCOM AND F.PD_NUMCOM = R.RD_NUMCOM
              AND F.PD_TIPOCOM = R.RD_TIPOCOM AND F.PD_PROGR = R.RD_PROGRFASE AND F.PD_CODMAC = R.RD_CODMAC
         WHERE C.CE_IDAZIENDA = ",
    );
    let a = s.arg(az);
    s.push(&a);
    let tutte = codici.len() == 1 && codici[0] == "*";
    if tutte {
        // MACCHINE=*: every machine resource, by code
        s.push(" AND C.CE_CANCELLATO = 'N' AND C.CE_FLGFAMIGLIA = 'N' AND C.CE_TIPORISORSA = 'M'");
    } else {
        if codici.is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<String> = codici.iter().map(|c| s.arg(c.as_str())).collect();
        s.push(&format!(" AND C.CODICE IN ({})", list.join(",")));
    }
    let t = db.query(s).await?;
    let mut by_cod: HashMap<String, Macchina> = HashMap::new();
    for r in &t.rows {
        let m = Macchina {
            cod: t.s(r, "CODICE"),
            descr: t.s(r, "DESCR"),
            famiglia: t.s(r, "CE_FAMIGLIA").trim().to_string(),
            operatore: t.s(r, "RD_CODDIP"),
            operatore_nome: t.s(r, "NOME"),
            anno: t.s(r, "RD_ANNOCOM"),
            num: t.s(r, "RD_NUMCOM"),
            tipo: t.s(r, "RD_TIPOCOM"),
            progr: t.s(r, "RD_PROGRFASE"),
            fase_descr: t.s(r, "PD_DESFAS"),
            versa_solo_buoni: t.s(r, "CE_VERSA_SOLO_BUONI") == "Y",
        };
        by_cod.insert(m.cod.clone(), m);
    }
    let mut out: Vec<Macchina> = if tutte {
        let mut v: Vec<Macchina> = by_cod.into_values().collect();
        v.sort_by(|a, b| a.cod.cmp(&b.cod));
        v
    } else {
        // Same order as the MACCHINE list
        codici.iter().filter_map(|c| by_cod.remove(c)).collect()
    };
    for m in &mut out {
        m.num = m.num.trim().to_string();
    }
    Ok(out)
}

/// RefreshAmbiente: the header line for the selected machine.
#[derive(Clone, Debug, Default)]
pub struct Ambiente {
    pub cod_dip: String,
    pub operatore: String,
    pub macchina: String,
    pub anno: String,
    pub num: String,
    pub tipo: String,
    pub progr: String,
    pub cod_fase: String,
    pub fase: String,
    pub articolo: String,
}

impl Ambiente {
    pub fn commessa_label(&self) -> String {
        if self.anno.is_empty() {
            return String::new();
        }
        let r = if self.tipo == "R" { "R " } else { "" };
        format!("{r}{} - {}", self.anno, self.num)
    }
}

pub async fn ambiente(db: &Db, az: &str, mac: &str) -> Result<Ambiente, String> {
    let mut s = Sql::new("");
    let m = s.arg(mac);
    let a = s.arg(az);
    s.text = format!(
        "SELECT R.RD_CODDIP, R.RD_ANNOCOM, R.RD_NUMCOM, R.RD_TIPOCOM, R.RD_PROGRFASE, M.DESCR, O.NOME, F.PD_CODFAS, F.PD_DESFAS,
           (SELECT TOP 1 D.PD_CODART FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = R.RD_IDAZIENDA AND D.PD_ANNOCOM = R.RD_ANNOCOM AND D.PD_NUMCOM = R.RD_NUMCOM AND D.PD_TIPOCOM = R.RD_TIPOCOM) AS CODART,
           C.PD_NOTE
         FROM RD_MACCHINE R
         LEFT OUTER JOIN CELPRO M ON (R.RD_CODMAC = M.CODICE AND M.CE_IDAZIENDA = R.RD_IDAZIENDA)
         LEFT OUTER JOIN Users O ON (O.USERNAME = R.RD_CODDIP AND O.IDAZIENDA = R.RD_IDAZIENDA)
         LEFT OUTER JOIN PD_COMM_T C ON (C.PD_ANNOCOM = R.RD_ANNOCOM AND C.PD_NUMCOM = R.RD_NUMCOM AND C.PD_TIPOCOM = R.RD_TIPOCOM AND C.PD_IDAZIENDA = R.RD_IDAZIENDA)
         LEFT OUTER JOIN PD_FASI F ON (F.PD_ANNOCOM = R.RD_ANNOCOM AND F.PD_IDAZIENDA = R.RD_IDAZIENDA AND F.PD_NUMCOM = R.RD_NUMCOM
              AND F.PD_TIPOCOM = R.RD_TIPOCOM AND R.RD_PROGRFASE = F.PD_PROGR AND R.RD_CODMAC = F.PD_CODMAC)
         WHERE R.RD_CODMAC = {m} AND R.RD_IDAZIENDA = {a}"
    );
    let t = db.query(s).await?;
    let Some(r) = t.rows.first() else { return Ok(Ambiente::default()) };
    let mut amb = Ambiente {
        cod_dip: t.s(r, "RD_CODDIP"),
        operatore: t.s(r, "NOME"),
        macchina: t.s(r, "DESCR"),
        anno: t.s(r, "RD_ANNOCOM"),
        num: t.s(r, "RD_NUMCOM"),
        tipo: t.s(r, "RD_TIPOCOM"),
        progr: t.s(r, "RD_PROGRFASE"),
        cod_fase: t.s(r, "PD_CODFAS"),
        fase: t.s(r, "PD_DESFAS"),
        articolo: String::new(),
    };
    let codart = t.s(r, "CODART");
    if !amb.num.is_empty() && !codart.is_empty() {
        let art = articoli(db, az, &[codart.clone()]).await.unwrap_or_default();
        let descr = art.get(&codart).map(|a| a.descr_commessa(&t.s(r, "PD_NOTE"))).unwrap_or_default();
        let descr = descr.replace("\r\n", " ").replace(['\r', '\n'], " ");
        let mut txt = format!("{codart} - {descr}");
        if txt.chars().count() > 75 {
            txt = txt.chars().take(74).collect();
        }
        amb.articolo = txt;
    }
    Ok(amb)
}

#[derive(Clone, Debug, Default)]
pub struct Articolo {
    pub descr: String,
    pub um: String,
    pub riga_libera: bool,
    pub decimali: Option<usize>,
}

impl Articolo {
    /// GestoreCommesse.GetDescrArticoloCommessa: free-text items take the order note.
    pub fn descr_commessa(&self, nota: &str) -> String {
        if self.riga_libera {
            if nota.trim().is_empty() { self.descr.trim().to_string() } else { nota.to_string() }
        } else {
            self.descr.trim().to_string()
        }
    }
}

pub async fn articoli(db: &Db, az: &str, codici: &[String]) -> Result<HashMap<String, Articolo>, String> {
    let mut out = HashMap::new();
    if codici.is_empty() {
        return Ok(out);
    }
    let mut s = Sql::new("SELECT MG_CODI, MG_DESC, MG_UNMIS, MG_RIGALIBERA, DECIMALIQTA FROM MG_ARTICOLI WHERE MG_IDAZIENDA = ");
    let a = s.arg(az);
    s.push(&a);
    let list: Vec<String> = codici.iter().map(|c| s.arg(c.as_str())).collect();
    s.push(&format!(" AND MG_CODI IN ({})", list.join(",")));
    let t = db.query(s).await?;
    for r in &t.rows {
        out.insert(
            t.s(r, "MG_CODI"),
            Articolo {
                descr: t.s(r, "MG_DESC"),
                um: t.s(r, "MG_UNMIS"),
                riga_libera: t.s(r, "MG_RIGALIBERA") == "Y",
                decimali: t.get(r, "DECIMALIQTA").i64().map(|d| d as usize),
            },
        );
    }
    Ok(out)
}

/// A scanned order code: optional year part and number part ("105266", "26-20105266",
/// "2026/20105266"). None when the number part is shorter than 3 digits.
pub fn analizza_codice(codice: &str) -> Option<(Option<String>, String)> {
    let c: String = codice.trim().chars().filter(|ch| ch.is_ascii_digit() || *ch == '-' || *ch == '/').collect();
    let (anno, num) = match c.rsplit_once(['-', '/']) {
        Some((a, n)) => (Some(a.trim_start_matches('0').to_string()).filter(|a| !a.is_empty()), n.to_string()),
        None => (None, c),
    };
    if num.len() < 3 { None } else { Some((anno, num)) }
}

/// Rank of an order number for a scanned number: 0 exact, 1 ends with, 2 contains.
pub fn rango(numero: &str, num: &str) -> Option<u8> {
    let n = numero.trim();
    if n == num {
        Some(0)
    } else if n.ends_with(num) {
        Some(1)
    } else if n.contains(num) {
        Some(2)
    } else {
        None
    }
}

/// A single order found by a scan, with its group order and the machines of this
/// workstation that have an open phase of the group.
#[derive(Clone, Debug)]
pub struct Trovata {
    pub anno: String,
    pub num: String,
    pub anno_r: String,
    pub num_r: String,
    pub codart: String,
    pub descr: String,
    pub qta: f64,
    pub um: String,
    pub macchine: Vec<String>,
    rango: u8,
}

/// Open single orders whose number matches the scan, grouped into an open order with
/// an open phase on one of `postazione` (machines or their families).
pub async fn cerca_singola(db: &Db, az: &str, codice: &str, postazione: &[Macchina]) -> Result<Vec<Trovata>, String> {
    let Some((anno, num)) = analizza_codice(codice) else { return Ok(Vec::new()) };
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_pat = s.arg(format!("%{num}%"));
    let anno_f = match &anno {
        Some(a) => {
            let p = s.arg(format!("%{a}"));
            format!(" AND S.PD_ANNOCOM LIKE {p}")
        }
        None => String::new(),
    };
    // One machine (the one being controlled): the group must also pass the original's
    // FaseDal.CheckFasiNonChiuseCommeMac (RD.Dal.MSSql 0xbde4), as in RefreshCommesse
    // (0x1cce8 IL_0337): an unfinished phase on the machine or its family whose
    // progressive is not already finished on this machine.
    let check_mac = match postazione {
        [m] => {
            let p_m = s.arg(m.cod.as_str());
            let p_f = s.arg(m.famiglia.as_str());
            format!(
                " AND EXISTS (SELECT 1 FROM PD_FASI F2 WHERE F2.PD_IDAZIENDA = S.PD_IDAZIENDA AND F2.PD_ANNOCOM = S.PD_ANNOCOMRAG AND F2.PD_NUMCOM = S.PD_NUMCOMRAG
                     AND F2.PD_TIPOCOM = 'R' AND (F2.PD_CODMAC = {p_m} OR F2.PD_CODMAC = {p_f}) AND F2.PD_DTEFIN IS NULL
                     AND NOT F2.PD_PROGR IN (SELECT F3.PD_PROGR FROM PD_FASI F3 WHERE F3.PD_IDAZIENDA = S.PD_IDAZIENDA AND F3.PD_ANNOCOM = S.PD_ANNOCOMRAG
                       AND F3.PD_NUMCOM = S.PD_NUMCOMRAG AND F3.PD_TIPOCOM = 'R' AND F3.PD_CODMAC = {p_m} AND NOT F3.PD_DTEFIN IS NULL))"
            )
        }
        _ => String::new(),
    };
    s.text = format!(
        "SELECT TOP 60 S.PD_ANNOCOM, S.PD_NUMCOM, S.PD_ANNOCOMRAG, S.PD_NUMCOMRAG,
           (SELECT TOP 1 D.PD_CODART FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = S.PD_IDAZIENDA AND D.PD_ANNOCOM = S.PD_ANNOCOM AND D.PD_NUMCOM = S.PD_NUMCOM AND D.PD_TIPOCOM = 'S') AS CODART,
           (SELECT SUM(D.PD_QUANTI) FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = S.PD_IDAZIENDA AND D.PD_ANNOCOM = S.PD_ANNOCOM AND D.PD_NUMCOM = S.PD_NUMCOM AND D.PD_TIPOCOM = 'S') AS QTA,
           STUFF((SELECT DISTINCT ',' + RTRIM(F.PD_CODMAC) FROM PD_FASI F
                  WHERE F.PD_IDAZIENDA = S.PD_IDAZIENDA AND F.PD_ANNOCOM = S.PD_ANNOCOMRAG AND F.PD_NUMCOM = S.PD_NUMCOMRAG AND F.PD_TIPOCOM = 'R' AND F.PD_DTEFIN IS NULL
                  FOR XML PATH('')), 1, 1, '') AS MACS
         FROM PD_COMM_T S
         WHERE S.PD_IDAZIENDA = {p_az} AND S.PD_TIPOCOM = 'S' AND S.PD_DATAC IS NULL AND S.PD_ANNOCOMRAG IS NOT NULL AND S.PD_NUMCOMRAG IS NOT NULL
           AND CAST(CAST(S.PD_NUMCOM AS DECIMAL(18,0)) AS VARCHAR(20)) LIKE {p_pat}{anno_f}
           AND EXISTS (SELECT 1 FROM PD_COMM_T R WHERE R.PD_IDAZIENDA = S.PD_IDAZIENDA AND R.PD_ANNOCOM = S.PD_ANNOCOMRAG AND R.PD_NUMCOM = S.PD_NUMCOMRAG
                       AND R.PD_TIPOCOM = 'R' AND R.PD_DATAC IS NULL){check_mac}
         ORDER BY S.PD_ANNOCOM DESC, S.PD_NUMCOM"
    );
    let t = db.query(s).await?;
    let codici: Vec<String> = {
        let mut v: Vec<String> = t.rows.iter().map(|r| t.s(r, "CODART")).filter(|c| !c.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    };
    let arts = articoli(db, az, &codici).await.unwrap_or_default();
    let mut out: Vec<Trovata> = t
        .rows
        .iter()
        .filter_map(|r| {
            let numero = t.s(r, "PD_NUMCOM");
            let rg = rango(&numero, &num)?;
            let aperte: Vec<String> = t.s(r, "MACS").split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect();
            // workstation machines that work the group: directly or through their family
            let macchine: Vec<String> = postazione
                .iter()
                .filter(|m| aperte.iter().any(|a| *a == m.cod || (!m.famiglia.is_empty() && *a == m.famiglia)))
                .map(|m| m.cod.clone())
                .collect();
            if macchine.is_empty() {
                return None;
            }
            let codart = t.s(r, "CODART");
            let art = arts.get(&codart).cloned().unwrap_or_default();
            Some(Trovata {
                anno: t.s(r, "PD_ANNOCOM"),
                num: numero,
                anno_r: t.s(r, "PD_ANNOCOMRAG"),
                num_r: t.s(r, "PD_NUMCOMRAG"),
                descr: art.descr.trim().to_string(),
                um: art.um.trim().to_string(),
                codart,
                qta: cell_f(t.get(r, "QTA")),
                macchine,
                rango: rg,
            })
        })
        .collect();
    out.sort_by(|a, b| a.rango.cmp(&b.rango).then(b.anno.cmp(&a.anno)).then(a.num.cmp(&b.num)));
    Ok(out)
}

/// Pieces counted as done for one single order, per phase — what
/// Versamento.GetArticoliDaProdurre (RD.Logic 0x35270) subtracts when InsParziale is on:
/// VF/VS recorded on the phase (Funzioni.GetNumPezzi, no extra-cycle), minus the scrap of the
/// item on the phase when the machine records only good pieces (Scarto.GetQtaScartataArticoloFase).
pub async fn versato_singola(db: &Db, az: &str, setup: &Setup, anno: &str, num: &str, codart: &str, solo_buoni: bool) -> Result<HashMap<i64, f64>, String> {
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(anno);
    let p_n = s.arg(Param::Float(num.parse().unwrap_or(0.0)));
    let p_vf = s.arg(setup.vf.as_str());
    let p_vs = s.arg(setup.vs.as_str());
    let p_art = s.arg(codart);
    s.text = format!(
        "SELECT P.PD_FASE,
           SUM(CASE WHEN P.PD_CODOPE = {p_vf} OR P.PD_CODOPE = {p_vs} THEN P.PD_NPEZZI ELSE 0 END) AS N,
           SUM(CASE WHEN O.PD_SCARTO = 'I' AND P.PD_CODART = {p_art} THEN P.PD_NPEZZI ELSE 0 END) AS SCARTI
         FROM PD_DATIPROD P
         LEFT JOIN PD_OPERAZIONI O ON O.PD_CODOPE = P.PD_CODOPE AND O.PD_IDAZIENDA = P.PD_IDAZIENDA
         WHERE P.PD_IDAZIENDA = {p_az} AND P.PD_ANNOCOM = {p_a} AND P.PD_COMMESSA = {p_n} AND P.PD_TIPOCOM = 'S'
           AND P.PD_EXTRACICLO IS NULL
         GROUP BY P.PD_FASE"
    );
    let t = db.query(s).await?;
    Ok(t
        .rows
        .iter()
        .map(|r| {
            let fatto = cell_f(t.get(r, "N")) - if solo_buoni { cell_f(t.get(r, "SCARTI")) } else { 0.0 };
            (cell_i(t.get(r, "PD_FASE")), fatto)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// INFO COMMESSA: which order a scanned code belongs to (reads only, any machine)

#[derive(Clone, Debug)]
pub struct InfoFase {
    pub progr: i64,
    pub descr: String,
    pub macchina: String,
    /// Same rule as the phase list (stato_fase, RefreshFasi 0x1e750).
    pub stato: StatoFase,
    pub produzione: bool,
    /// Pieces recorded for the scanned single order on this phase (VF + VS).
    pub versato: f64,
}

#[derive(Clone, Debug)]
pub struct InfoCommessa {
    pub anno: String,
    pub num: String,
    pub chiusa: bool,
    pub anno_r: String,
    pub num_r: String,
    pub r_chiusa: bool,
    pub codart: String,
    pub descr: String,
    pub qta: f64,
    pub um: String,
    /// Machines whose state (RD_MACCHINE) has the group order on them now.
    pub su_macchine: Vec<String>,
    pub fasi: Vec<InfoFase>,
    rango: u8,
}

/// Single orders (open or closed) whose number matches the scan and whose group order has
/// phases on the workstation's machines (Esprit.ini MACCHINE, or their families): which
/// machine of this workstation the order is on. Details for the best 5 matches.
pub async fn info_codice(db: &Db, az: &str, setup: &Setup, codice: &str, postazione: &[Macchina]) -> Result<Vec<InfoCommessa>, String> {
    let Some((anno, num)) = analizza_codice(codice) else { return Ok(Vec::new()) };
    // machine codes of the workstation and their families, as PD_FASI.PD_CODMAC may hold either
    // (RefreshCommesse 0x1cce8 IL_00c2: PD_CODMAC = @PD_CODMAC OR PD_CODMAC = @PD_CODFAMIGLIA).
    // `postazione` is MACCHINE of Esprit.ini, "*" = every machine (ESP_FormMain::.ctor 0x1a054).
    let mut codici_mac: Vec<String> = postazione.iter().flat_map(|m| [m.cod.trim().to_string(), m.famiglia.trim().to_string()]).filter(|c| !c.is_empty()).collect();
    codici_mac.sort();
    codici_mac.dedup();
    if codici_mac.is_empty() {
        return Ok(Vec::new());
    }
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_pat = s.arg(format!("%{num}%"));
    let anno_f = match &anno {
        Some(a) => {
            let p = s.arg(format!("%{a}"));
            format!(" AND S.PD_ANNOCOM LIKE {p}")
        }
        None => String::new(),
    };
    let in_mac = codici_mac.iter().map(|c| s.arg(c.as_str())).collect::<Vec<_>>().join(",");
    s.text = format!(
        "SELECT TOP 40 S.PD_ANNOCOM, S.PD_NUMCOM, S.PD_ANNOCOMRAG, S.PD_NUMCOMRAG,
           CASE WHEN S.PD_DATAC IS NULL THEN 'N' ELSE 'Y' END AS CHIUSA,
           (SELECT TOP 1 CASE WHEN R.PD_DATAC IS NULL THEN 'N' ELSE 'Y' END FROM PD_COMM_T R WHERE R.PD_IDAZIENDA = S.PD_IDAZIENDA
              AND R.PD_ANNOCOM = S.PD_ANNOCOMRAG AND R.PD_NUMCOM = S.PD_NUMCOMRAG AND R.PD_TIPOCOM = 'R') AS R_CHIUSA,
           (SELECT TOP 1 D.PD_CODART FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = S.PD_IDAZIENDA AND D.PD_ANNOCOM = S.PD_ANNOCOM AND D.PD_NUMCOM = S.PD_NUMCOM AND D.PD_TIPOCOM = 'S') AS CODART,
           (SELECT SUM(D.PD_QUANTI) FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = S.PD_IDAZIENDA AND D.PD_ANNOCOM = S.PD_ANNOCOM AND D.PD_NUMCOM = S.PD_NUMCOM AND D.PD_TIPOCOM = 'S') AS QTA
         FROM PD_COMM_T S
         WHERE S.PD_IDAZIENDA = {p_az} AND S.PD_TIPOCOM = 'S'
           AND CAST(CAST(S.PD_NUMCOM AS DECIMAL(18,0)) AS VARCHAR(20)) LIKE {p_pat}{anno_f}
           AND EXISTS (SELECT 1 FROM PD_FASI F WHERE F.PD_IDAZIENDA = S.PD_IDAZIENDA AND F.PD_ANNOCOM = S.PD_ANNOCOMRAG
                       AND F.PD_NUMCOM = S.PD_NUMCOMRAG AND F.PD_TIPOCOM = 'R' AND F.PD_CODMAC IN ({in_mac}))
         ORDER BY S.PD_ANNOCOM DESC, S.PD_NUMCOM"
    );
    let t = db.query(s).await?;
    let mut out: Vec<InfoCommessa> = t
        .rows
        .iter()
        .filter_map(|r| {
            let numero = t.s(r, "PD_NUMCOM");
            let rg = rango(&numero, &num)?;
            Some(InfoCommessa {
                anno: t.s(r, "PD_ANNOCOM"),
                num: numero,
                chiusa: t.s(r, "CHIUSA") == "Y",
                anno_r: t.s(r, "PD_ANNOCOMRAG"),
                num_r: t.s(r, "PD_NUMCOMRAG"),
                r_chiusa: t.s(r, "R_CHIUSA") == "Y",
                codart: t.s(r, "CODART"),
                descr: String::new(),
                qta: cell_f(t.get(r, "QTA")),
                um: String::new(),
                su_macchine: Vec::new(),
                fasi: Vec::new(),
                rango: rg,
            })
        })
        .collect();
    // open orders first, then the best match
    out.sort_by(|a, b| a.chiusa.cmp(&b.chiusa).then(a.rango.cmp(&b.rango)).then(b.anno.cmp(&a.anno)).then(a.num.cmp(&b.num)));
    out.truncate(5);
    let codici: Vec<String> = out.iter().map(|i| i.codart.clone()).filter(|c| !c.is_empty()).collect();
    let arts = articoli(db, az, &codici).await.unwrap_or_default();
    for i in &mut out {
        if let Some(a) = arts.get(&i.codart) {
            i.descr = a.descr.trim().to_string();
            i.um = a.um.trim().to_string();
        }
        if i.anno_r.is_empty() || i.num_r.is_empty() {
            continue;
        }
        // phases of the group order on the workstation's machines (or their families)
        let mut s = Sql::new("");
        let p_az = s.arg(az);
        let p_a = s.arg(i.anno_r.as_str());
        let p_n = s.arg(Param::Float(i.num_r.parse().unwrap_or(0.0)));
        let in_mac = codici_mac.iter().map(|c| s.arg(c.as_str())).collect::<Vec<_>>().join(",");
        s.text = format!(
            "SELECT F.PD_PROGR, F.PD_DESFAS, F.PD_CODMAC, F.PD_DTEINI, F.PD_DTESOSP, F.PD_DTEFIN, T.PD_TYP
             FROM PD_FASI F LEFT JOIN PD_TIPIFASI T ON T.PD_CODFAS = F.PD_CODFAS AND T.PD_IDAZIENDA = F.PD_IDAZIENDA
             WHERE F.PD_IDAZIENDA = {p_az} AND F.PD_ANNOCOM = {p_a} AND F.PD_NUMCOM = {p_n} AND F.PD_TIPOCOM = 'R'
               AND ISNULL(T.PD_TYP, 0) != 5 AND F.PD_CODMAC IN ({in_mac})
             ORDER BY F.PD_PROGR, F.PD_CODMAC"
        );
        let versato = versato_singola(db, az, setup, &i.anno, &i.num, &i.codart, false).await.unwrap_or_default();
        if let Ok(ft) = db.query(s).await {
            i.fasi = ft
                .rows
                .iter()
                .map(|r| {
                    let progr = cell_i(ft.get(r, "PD_PROGR"));
                    let stato = stato_fase(!ft.get(r, "PD_DTEINI").is_null(), !ft.get(r, "PD_DTESOSP").is_null(), !ft.get(r, "PD_DTEFIN").is_null());
                    let typ = cell_i(ft.get(r, "PD_TYP"));
                    // the workstation machine(s) behind the phase's machine or family code
                    let cod = ft.s(r, "PD_CODMAC").trim().to_string();
                    let nomi: Vec<String> = postazione
                        .iter()
                        .filter(|m| m.cod.trim() == cod || m.famiglia.trim() == cod)
                        .map(|m| if m.descr.trim().is_empty() { m.cod.trim().to_string() } else { m.descr.trim().to_string() })
                        .collect();
                    let macchina = if postazione.iter().any(|m| m.cod.trim() == cod) || nomi.is_empty() { nomi.first().cloned().unwrap_or(cod) } else { nomi.join(" / ") };
                    InfoFase {
                        progr,
                        descr: ft.s(r, "PD_DESFAS").trim().to_string(),
                        macchina,
                        stato,
                        produzione: typ == 2 || typ == 4,
                        versato: versato.get(&progr).copied().unwrap_or(0.0),
                    }
                })
                .collect();
        }
        // machines working the group now
        let mut s = Sql::new("SELECT RD_CODMAC FROM RD_MACCHINE WHERE RD_IDAZIENDA = ");
        let a = s.arg(az);
        s.push(&a);
        s.and("AND RD_ANNOCOM = ? AND RD_NUMCOM = ? AND RD_TIPOCOM = 'R'", vec![i.anno_r.as_str().into(), Param::Float(i.num_r.parse().unwrap_or(0.0))]);
        if let Ok(mt) = db.query(s).await {
            i.su_macchine = mt
                .rows
                .iter()
                .filter_map(|r| {
                    let c = mt.s(r, "RD_CODMAC").trim().to_string();
                    postazione.iter().find(|m| m.cod.trim() == c).map(|m| if m.descr.trim().is_empty() { c.clone() } else { m.descr.trim().to_string() })
                })
                .collect();
        }
    }
    Ok(out)
}

/// Funzioni.getDescrClass1
pub async fn descr_class1(db: &Db, az: &str, codici: &[String]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    if codici.is_empty() {
        return Ok(out);
    }
    let mut s = Sql::new("SELECT COD, DESCRIZIONE FROM MG_CLASS1 WHERE IDAZIENDA = ");
    let a = s.arg(az);
    s.push(&a);
    let list: Vec<String> = codici.iter().map(|c| s.arg(c.as_str())).collect();
    s.push(&format!(" AND COD IN ({})", list.join(",")));
    let t = db.query(s).await?;
    for r in &t.rows {
        out.insert(t.s(r, "COD"), t.s(r, "DESCRIZIONE"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Operators (RefreshOperatori)

#[derive(Clone, Debug)]
pub struct Turno {
    pub cod: String,
    pub nome: String,
    pub inizio: Option<NaiveDateTime>,
}

pub async fn turni(db: &Db, az: &str) -> Result<Vec<Turno>, String> {
    let mut s = Sql::new("SELECT ID, USERNAME, NOME, RD_ORAINI, RD_DATAINI FROM RD_TURNI, Users WHERE RD_OPERATORE = USERNAME AND RD_ORAFIN IS NULL AND RD_IDAZIENDA = ");
    let a = s.arg(az);
    s.push(&a);
    let t = db.query(s).await?;
    Ok(t.rows
        .iter()
        .map(|r| {
            let d = cell_date(t.get(r, "RD_DATAINI"));
            let o = cell_time(t.get(r, "RD_ORAINI"));
            Turno {
                cod: t.s(r, "USERNAME"),
                nome: t.s(r, "NOME"),
                inizio: d.map(|d| d.and_time(o.unwrap_or_default())),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Orders (RefreshCommesse + CommessaRD + Funzioni.getColoreCommessa)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Semaforo {
    Verde,
    Giallo,
    Azzurro,
    Bianco,
    Rosso,
}

impl Semaforo {
    /// GetOrderingColoreCommessa
    pub fn ordine(self) -> i32 {
        match self {
            Semaforo::Verde => 1,
            Semaforo::Giallo => 2,
            Semaforo::Azzurro => 3,
            Semaforo::Bianco => 4,
            Semaforo::Rosso => 5,
        }
    }

    /// Code sent to the save helper ("Y"/"G" may start without asking).
    pub fn codice(self) -> &'static str {
        match self {
            Semaforo::Verde => "G",
            Semaforo::Giallo => "Y",
            Semaforo::Azzurro => "B",
            Semaforo::Bianco => "W",
            Semaforo::Rosso => "R",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Semaforo::Verde => "In corso su questa macchina",
            Semaforo::Giallo => "Pronta a partire",
            Semaforo::Azzurro => "In attesa delle commesse precedenti",
            Semaforo::Bianco => "Fase terminata",
            Semaforo::Rosso => "Non pronta",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Commessa {
    pub anno: String,
    pub num: String,
    pub tipo: String,
    pub annotazione: String,
    pub note: String,
    pub datcom: Option<NaiveDate>,
    pub dt_consegna: Option<NaiveDate>,
    pub priorita: i64,
    pub ord_lancio: i64,
    pub codart: String,
    pub descr_art: String,
    pub um: String,
    pub qta: f64,
    pub prodotti: f64,
    pub prodotti_tot: f64,
    pub scarti: f64,
    pub semaforo: Semaforo,
}

impl Commessa {
    pub fn codice(&self) -> String {
        let r = if self.tipo == "R" { "R " } else { "" };
        format!("{r}{}-{}", self.anno, self.num)
    }

    pub fn chiave(&self) -> (String, String, String) {
        (self.anno.clone(), self.num.clone(), self.tipo.clone())
    }
}

pub struct FiltroCommesse {
    pub anno: Option<i32>,
    pub num: Option<i64>,
}

pub async fn commesse(
    db: &Db,
    az: &str,
    setup: &Setup,
    mac: &Macchina,
    filtro: &FiltroCommesse,
    max_note: usize,
) -> Result<Vec<Commessa>, String> {
    let fam = mac.famiglia.clone();
    // The filter "Anno-Numero" may name a single order: use its grouping order instead.
    let (fa, fnum) = match (filtro.anno, filtro.num) {
        (Some(a), Some(n)) => {
            let mut s = Sql::new("SELECT PD_ANNOCOMRAG, PD_NUMCOMRAG FROM PD_COMM_T WHERE PD_IDAZIENDA = ");
            let p = s.arg(az);
            s.push(&p);
            s.and("AND PD_ANNOCOM = ? AND PD_NUMCOM = ? AND PD_TIPOCOM = 'S' AND PD_ANNOCOMRAG IS NOT NULL", vec![a.to_string().into(), Param::Float(n as f64)]);
            let t = db.query(s).await?;
            match t.rows.first() {
                Some(r) => (Some(t.s(r, "PD_ANNOCOMRAG")), Some(t.get(r, "PD_NUMCOMRAG").i64().unwrap_or(n))),
                None => (Some(a.to_string()), Some(n)),
            }
        }
        _ => (None, None),
    };
    let limite = (Local::now().date_naive() + chrono::Duration::days(setup.days_show)).and_hms_opt(0, 0, 0).unwrap();

    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_mac = s.arg(mac.cod.as_str());
    let p_fam = s.arg(fam.as_str());
    let p_lim = s.arg(limite);
    let p_vf = s.arg(setup.vf.as_str());
    let mut extra = String::new();
    if let (Some(a), Some(n)) = (&fa, fnum) {
        let pa = s.arg(a.as_str());
        let pn = s.arg(Param::Float(n as f64));
        extra = format!(" AND T.PD_ANNOCOM = {pa} AND T.PD_NUMCOM = {pn}");
    }
    // List query of RefreshCommesse, CheckFasiNonChiuseCommeMac and the card values
    // of CommessaRD (articolo, quantità, prodotti, scarti) in one go.
    s.text = format!(
        "WITH L AS (
           SELECT DISTINCT T.PD_ANNOCOM, T.PD_NUMCOM, T.PD_TIPOCOM, T.PD_ORDLANCIO, T.PD_ANNOTAZIONE, T.PD_PRIORLANCIO, T.PD_DATCOM, T.PD_NOTE, T.PD_DATACONSEGNA
           FROM PD_COMM_T AS T INNER JOIN PD_FASI AS F ON T.PD_IDAZIENDA = F.PD_IDAZIENDA AND T.PD_ANNOCOM = F.PD_ANNOCOM AND T.PD_NUMCOM = F.PD_NUMCOM AND T.PD_TIPOCOM = F.PD_TIPOCOM
           WHERE T.PD_DATAC IS NULL AND T.PD_IDAZIENDA = {p_az} AND (T.PD_DATCOM <= {p_lim} OR T.PD_PRIORLANCIO < 999) AND T.PD_TIPOCOM = 'R'
             AND (F.PD_CODMAC = {p_mac} OR F.PD_CODMAC = {p_fam}) AND F.PD_DTEFIN IS NULL{extra}
         )
         SELECT L.*,
           (SELECT TOP 1 D.PD_CODART FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = {p_az} AND D.PD_ANNOCOM = L.PD_ANNOCOM AND D.PD_NUMCOM = L.PD_NUMCOM AND D.PD_TIPOCOM = L.PD_TIPOCOM) AS CODART,
           (SELECT SUM(D.PD_QUANTI) FROM PD_COMM_D D WHERE D.PD_IDAZIENDA = {p_az} AND D.PD_ANNOCOM = L.PD_ANNOCOM AND D.PD_NUMCOM = L.PD_NUMCOM AND D.PD_TIPOCOM = L.PD_TIPOCOM) AS QTA,
           (SELECT SUM(P.PD_NPEZZI) FROM PD_DATIPROD P WHERE P.PD_EXTRACICLO IS NULL AND P.PD_CODOPE = {p_vf} AND P.PD_ANNOCOMRAG = L.PD_ANNOCOM AND P.PD_NUMCOMRAG = L.PD_NUMCOM AND P.PD_TIPOCOM = 'S' AND P.PD_IDAZIENDA = {p_az} AND P.PD_MACCHINA = {p_mac}) AS PROD_MAC,
           (SELECT SUM(P.PD_NPEZZI) FROM PD_DATIPROD P WHERE P.PD_EXTRACICLO IS NULL AND P.PD_CODOPE = {p_vf} AND P.PD_ANNOCOMRAG = L.PD_ANNOCOM AND P.PD_NUMCOMRAG = L.PD_NUMCOM AND P.PD_TIPOCOM = 'S' AND P.PD_IDAZIENDA = {p_az}) AS PROD_TOT,
           (SELECT SUM(P.PD_NPEZZI) FROM PD_DATIPROD P INNER JOIN PD_OPERAZIONI O ON P.PD_CODOPE = O.PD_CODOPE AND P.PD_IDAZIENDA = O.PD_IDAZIENDA
              WHERE P.PD_EXTRACICLO IS NULL AND O.PD_SCARTO = 'I' AND P.PD_ANNOCOMRAG = L.PD_ANNOCOM AND P.PD_NUMCOMRAG = L.PD_NUMCOM AND P.PD_TIPOCOM = 'S' AND P.PD_IDAZIENDA = {p_az}) AS SCARTI,
           (SELECT COUNT(*) FROM RD_MACCHINE R WHERE R.RD_ANNOCOM = L.PD_ANNOCOM AND R.RD_NUMCOM = L.PD_NUMCOM AND R.RD_TIPOCOM = L.PD_TIPOCOM AND R.RD_CODMAC = {p_mac} AND R.RD_IDAZIENDA = {p_az}) AS IN_CORSO
         FROM L
         WHERE EXISTS (
           SELECT 1 FROM PD_FASI F2 WHERE F2.PD_IDAZIENDA = {p_az} AND F2.PD_ANNOCOM = L.PD_ANNOCOM AND F2.PD_NUMCOM = L.PD_NUMCOM AND F2.PD_TIPOCOM = L.PD_TIPOCOM
             AND (F2.PD_CODMAC = {p_mac} OR F2.PD_CODMAC = {p_fam}) AND F2.PD_DTEFIN IS NULL
             AND NOT F2.PD_PROGR IN (SELECT F3.PD_PROGR FROM PD_FASI F3 WHERE F3.PD_IDAZIENDA = {p_az} AND F3.PD_ANNOCOM = L.PD_ANNOCOM AND F3.PD_NUMCOM = L.PD_NUMCOM
               AND F3.PD_TIPOCOM = L.PD_TIPOCOM AND F3.PD_CODMAC = {p_mac} AND NOT F3.PD_DTEFIN IS NULL))"
    );
    let t = db.query(s).await?;
    let codici: Vec<String> = {
        let mut v: Vec<String> = t.rows.iter().map(|r| t.s(r, "CODART")).filter(|c| !c.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    };
    let arts = articoli(db, az, &codici).await.unwrap_or_default();
    // CommessaRD.CodArticolo: without an item description, the class-1 description (MG_CLASS1)
    let senza: Vec<String> = codici.iter().filter(|c| arts.get(*c).map(|a| a.descr.trim().is_empty()).unwrap_or(true)).cloned().collect();
    let classi = descr_class1(db, az, &senza).await.unwrap_or_default();

    let mut out = Vec::new();
    for r in &t.rows {
        let note_full = t.s(r, "PD_NOTE");
        let mut note = String::new();
        if max_note >= 3 {
            note = note_full.trim_end().to_string();
            if note.chars().count() > max_note && note.chars().count() > 3 {
                note = note.chars().take(max_note - 3).collect::<String>() + "...";
            }
            note = note.replace("\r\n", " ");
        }
        let codart = t.s(r, "CODART");
        let art = arts.get(&codart).cloned().unwrap_or_default();
        let semaforo = if cell_i(t.get(r, "IN_CORSO")) > 0 { Semaforo::Verde } else { Semaforo::Bianco };
        let mut c = Commessa {
            anno: t.s(r, "PD_ANNOCOM"),
            num: t.s(r, "PD_NUMCOM"),
            tipo: t.s(r, "PD_TIPOCOM"),
            annotazione: t.s(r, "PD_ANNOTAZIONE"),
            note,
            datcom: cell_date(t.get(r, "PD_DATCOM")),
            dt_consegna: cell_date(t.get(r, "PD_DATACONSEGNA")),
            priorita: cell_i(t.get(r, "PD_PRIORLANCIO")),
            ord_lancio: t.get(r, "PD_ORDLANCIO").i64().unwrap_or(9_999_999_999),
            descr_art: {
                let d = art.descr_commessa(&note_full);
                if d.trim().is_empty() { classi.get(&codart).cloned().unwrap_or_default() } else { d }
            },
            um: art.um.clone(),
            codart,
            qta: cell_f(t.get(r, "QTA")),
            prodotti: cell_f(t.get(r, "PROD_MAC")),
            prodotti_tot: cell_f(t.get(r, "PROD_TOT")),
            scarti: cell_f(t.get(r, "SCARTI")),
            semaforo,
        };
        if mac.versa_solo_buoni {
            c.prodotti -= c.scarti;
            c.prodotti_tot -= c.scarti;
        }
        out.push(c);
    }

    // Traffic lights for orders not running on this machine
    let futs = out.iter().map(|c| {
        let db = db.clone();
        let az = az.to_string();
        let mac = mac.cod.clone();
        let fam = fam.clone();
        let setup = setup.clone();
        let c = c.clone();
        async move {
            if c.semaforo == Semaforo::Verde {
                return Ok(Semaforo::Verde);
            }
            colore_commessa(&db, &az, &setup, &c, &mac, &fam).await
        }
    });
    let colori: Vec<Result<Semaforo, String>> = futures_util::future::join_all(futs).await;
    for (c, col) in out.iter_mut().zip(colori) {
        c.semaforo = col?;
    }
    // BLOCCA_SE_SEMAFORO_ROSSO hides orders that are neither green nor yellow
    if setup.blocca_rosso {
        out.retain(|c| matches!(c.semaforo, Semaforo::Verde | Semaforo::Giallo));
    }
    ordina_commesse(&mut out, &setup.ord_comme);
    if setup.max_commesse > 0 && setup.max_commesse < out.len() {
        out.truncate(setup.max_commesse);
    }
    Ok(out)
}

/// The sort of RefreshCommesse.
pub fn ordina_commesse(v: &mut [Commessa], ord: &str) {
    v.sort_by(|a, b| {
        a.semaforo
            .ordine()
            .cmp(&b.semaforo.ordine())
            .then(a.ord_lancio.cmp(&b.ord_lancio))
            .then_with(|| match ord {
                "P" => a.priorita.cmp(&b.priorita).then(a.datcom.cmp(&b.datcom)),
                "D" => a.datcom.cmp(&b.datcom).then(a.priorita.cmp(&b.priorita)),
                _ => std::cmp::Ordering::Equal,
            })
            .then(a.anno.cmp(&b.anno))
            .then(a.num.cmp(&b.num))
    });
}

/// Funzioni.getColoreCommessa for an order not running on the machine.
async fn colore_commessa(db: &Db, az: &str, setup: &Setup, c: &Commessa, mac: &str, fam: &str) -> Result<Semaforo, String> {
    // getCommessePrecedenti of a grouped order: the singles (PD_COMM_D ANNOCOMPRV/NUMCOMPRV
    // in order), each with its PD_COMM_PREC rows. Per row: started?, quantity, produced, closed.
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(c.anno.as_str());
    let p_n = s.arg(Param::Float(c.num.parse::<f64>().unwrap_or(0.0)));
    let p_t = s.arg(c.tipo.as_str());
    let p_vf = s.arg(setup.vf.as_str());
    s.text = format!(
        "WITH SG AS (
           SELECT DISTINCT PD_ANNOCOMPRV, PD_NUMCOMPRV FROM PD_COMM_D WHERE PD_IDAZIENDA = {p_az} AND PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t}
         )
         SELECT SG.PD_ANNOCOMPRV, SG.PD_NUMCOMPRV, P.PD_TIPOCOMPREC, P.PD_ANNOCOMPREC, P.PD_NUMCOMPREC, ISNULL(P.PD_PERCENTUALE, 0) AS PERC,
           MP.MA, MP.MN, MP.MT,
           (SELECT COUNT(*) FROM PD_FASI F WHERE F.PD_ANNOCOM = MP.MA AND F.PD_NUMCOM = MP.MN AND F.PD_TIPOCOM = MP.MT AND NOT (F.PD_DTEINI IS NULL) AND F.PD_IDAZIENDA = {p_az}) AS INIZ,
           (SELECT SUM(D.PD_QUANTI) FROM PD_COMM_D D WHERE D.PD_ANNOCOM = P.PD_ANNOCOMPREC AND D.PD_NUMCOM = P.PD_NUMCOMPREC AND D.PD_TIPOCOM = P.PD_TIPOCOMPREC AND D.PD_IDAZIENDA = {p_az}) AS QTA,
           CASE WHEN ISNULL(P.PD_PERCENTUALE, 0) > 0 THEN
             CASE WHEN P.PD_TIPOCOMPREC = 'S' THEN
               (SELECT SUM(X.PD_NPEZZI) FROM PD_DATIPROD X WHERE X.PD_EXTRACICLO IS NULL AND X.PD_CODOPE = {p_vf} AND X.PD_ANNOCOM = P.PD_ANNOCOMPREC AND X.PD_COMMESSA = P.PD_NUMCOMPREC AND X.PD_TIPOCOM = 'S' AND X.PD_IDAZIENDA = {p_az})
             ELSE
               (SELECT SUM(X.PD_NPEZZI) FROM PD_DATIPROD X WHERE X.PD_EXTRACICLO IS NULL AND X.PD_CODOPE = {p_vf} AND X.PD_ANNOCOMRAG = P.PD_ANNOCOMPREC AND X.PD_NUMCOMRAG = P.PD_NUMCOMPREC AND X.PD_TIPOCOM = 'S' AND X.PD_IDAZIENDA = {p_az})
             END END AS PROD,
           (SELECT TOP 1 T.PD_FLAGC FROM PD_COMM_T T WHERE T.PD_ANNOCOM = MP.MA AND T.PD_NUMCOM = MP.MN AND T.PD_TIPOCOM = MP.MT AND T.PD_IDAZIENDA = {p_az}) AS FLAGC
         FROM SG
         INNER JOIN PD_COMM_PREC P ON P.PD_IDAZIENDA = {p_az} AND P.PD_TIPOCOM = 'S' AND P.PD_ANNOCOM = SG.PD_ANNOCOMPRV AND P.PD_NUMCOM = SG.PD_NUMCOMPRV
         OUTER APPLY (
           SELECT
             CASE WHEN P.PD_TIPOCOMPREC = 'S' AND R.PD_ANNOCOMRAG IS NOT NULL AND R.PD_NUMCOMRAG IS NOT NULL THEN R.PD_ANNOCOMRAG ELSE P.PD_ANNOCOMPREC END AS MA,
             CASE WHEN P.PD_TIPOCOMPREC = 'S' AND R.PD_ANNOCOMRAG IS NOT NULL AND R.PD_NUMCOMRAG IS NOT NULL THEN R.PD_NUMCOMRAG ELSE P.PD_NUMCOMPREC END AS MN,
             CASE WHEN P.PD_TIPOCOMPREC = 'S' AND R.PD_ANNOCOMRAG IS NOT NULL AND R.PD_NUMCOMRAG IS NOT NULL THEN 'R' ELSE P.PD_TIPOCOMPREC END AS MT
           FROM (SELECT 1 AS X) Z
           LEFT JOIN PD_COMM_T R ON R.PD_IDAZIENDA = {p_az} AND R.PD_ANNOCOM = P.PD_ANNOCOMPREC AND R.PD_NUMCOM = P.PD_NUMCOMPREC AND R.PD_TIPOCOM = P.PD_TIPOCOMPREC
         ) MP
         ORDER BY SG.PD_ANNOCOMPRV, SG.PD_NUMCOMPRV, P.PD_TIPOCOMPREC, P.PD_ANNOCOMPREC, P.PD_NUMCOMPREC"
    );
    let t = db.query(s).await?;
    let (pronta, iniziate) = valuta_precedenti(&t);
    let mut flag = pronta;
    if flag {
        flag = prima_fase_pronta(db, az, setup, c, mac, fam).await?;
    }
    Ok(if flag {
        Semaforo::Giallo
    } else if iniziate {
        Semaforo::Azzurro
    } else {
        Semaforo::Rosso
    })
}

/// The loop over the preceding orders: (all satisfied, first one started).
fn valuta_precedenti(t: &Table) -> (bool, bool) {
    if t.rows.is_empty() {
        return (true, true);
    }
    let mut iniziate = false;
    for r in &t.rows {
        if cell_i(t.get(r, "INIZ")) <= 0 {
            return (false, iniziate);
        }
        iniziate = true;
        let perc = cell_f(t.get(r, "PERC"));
        if perc > 0.0 {
            let qta = cell_f(t.get(r, "QTA"));
            let prod = cell_f(t.get(r, "PROD"));
            let num = if qta > 0.0 { prod / qta * 100.0 } else { 0.0 };
            if perc > num {
                return (false, iniziate);
            }
            if perc == 100.0 && t.s(r, "FLAGC") != "S" {
                return (false, iniziate);
            }
        }
    }
    (true, iniziate)
}

/// PrimaFaseSuMacchinaProntaAPartire
async fn prima_fase_pronta(db: &Db, az: &str, setup: &Setup, c: &Commessa, mac: &str, fam: &str) -> Result<bool, String> {
    let mut s = Sql::new("SELECT MIN(PD_PROGR) AS P FROM PD_FASI WHERE PD_IDAZIENDA = ");
    let a = s.arg(az);
    s.push(&a);
    s.and(
        "AND PD_ANNOCOM = ? AND PD_NUMCOM = ? AND PD_TIPOCOM = ? AND (PD_CODMAC = ? OR PD_CODMAC = ?) AND PD_DTEINI IS NULL",
        vec![c.anno.as_str().into(), Param::Float(c.num.parse().unwrap_or(0.0)), c.tipo.as_str().into(), mac.into(), fam.into()],
    );
    let t = db.query(s).await?;
    let Some(p) = t.rows.first().and_then(|r| t.get(r, "P").i64()) else { return Ok(true) };
    let col = colore_fase(db, az, setup, &c.anno, &c.num, &c.tipo, p, mac, c.qta).await?;
    Ok(col != Semaforo::Rosso)
}

/// Funzioni.getColoreFase
pub async fn colore_fase(db: &Db, az: &str, setup: &Setup, anno: &str, num: &str, tipo: &str, progr: i64, mac: &str, n_pezzi_da_prod: f64) -> Result<Semaforo, String> {
    let n: f64 = num.parse().unwrap_or(0.0);
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(anno);
    let p_n = s.arg(Param::Float(n));
    let p_t = s.arg(tipo);
    let p_p = s.arg(Param::Float(progr as f64));
    let p_m = s.arg(mac);
    let p_vf = s.arg(setup.vf.as_str());
    let p_vs = s.arg(setup.vs.as_str());
    let p_ore = s.arg(setup.ore.as_str());
    let mac_filter = if mac.is_empty() { String::new() } else { format!(" AND PD_CODMAC = {p_m}") };
    s.text = format!(
        "DECLARE @PREC DECIMAL(18,0) = (SELECT MAX(PD_PROGR) FROM PD_FASI WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_PROGR < {p_p} AND PD_IDAZIENDA = {p_az});
         SELECT
           (SELECT MAX(PD_DTEINI) FROM PD_FASI WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_PROGR = {p_p} AND PD_IDAZIENDA = {p_az}{mac_filter}) AS DTINI,
           (SELECT MAX(PD_DTEFIN) FROM PD_FASI WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_PROGR = {p_p} AND PD_IDAZIENDA = {p_az}{mac_filter}) AS DTFINE,
           (SELECT MAX(PD_PERCSOV) FROM PD_FASI WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_PROGR = {p_p} AND PD_IDAZIENDA = {p_az}{mac_filter}) AS PERCSOV,
           @PREC AS PREC,
           (SELECT MAX(PD_DTEFIN) FROM PD_FASI WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_PROGR = @PREC AND PD_IDAZIENDA = {p_az}) AS PREC_FINE,
           (SELECT SUM(PD_NPEZZI) FROM PD_DATIPROD WHERE PD_EXTRACICLO IS NULL AND (PD_CODOPE = {p_vf} OR PD_CODOPE = {p_vs})
              AND PD_ANNOCOMRAG = {p_a} AND PD_NUMCOMRAG = {p_n} AND PD_TIPOCOM = 'S' AND PD_IDAZIENDA = {p_az} AND PD_FASE = @PREC) AS PREC_PEZZI,
           (SELECT COUNT(*) FROM PD_DATIPROD WHERE PD_IDAZIENDA = {p_az} AND (PD_CODOPE IS NULL OR PD_CODOPE <> 'SPC') AND PD_TIPOCOM = {p_t}
              AND PD_ANNOCOM >= {p_a} AND PD_ANNOCOM <= {p_a} AND PD_COMMESSA >= {p_n} AND PD_COMMESSA <= {p_n}
              AND PD_FASE >= @PREC AND PD_FASE <= @PREC AND PD_CODOPE >= {p_ore} AND PD_CODOPE <= {p_ore}) AS PREC_ORE"
    );
    let t = db.query(s).await?;
    let Some(r) = t.rows.first() else { return Ok(Semaforo::Rosso) };
    if !t.get(r, "DTINI").is_null() {
        return Ok(if t.get(r, "DTFINE").is_null() { Semaforo::Verde } else { Semaforo::Bianco });
    }
    let perc = cell_f(t.get(r, "PERCSOV"));
    let pronta = if t.get(r, "PREC").is_null() {
        true
    } else if perc == 100.0 {
        cell_i(t.get(r, "PREC_ORE")) > 0
    } else {
        let pezzi = cell_f(t.get(r, "PREC_PEZZI"));
        let num2 = if n_pezzi_da_prod > 0.0 { pezzi / n_pezzi_da_prod * 100.0 } else { 0.0 };
        num2 >= 100.0 - perc || !t.get(r, "PREC_FINE").is_null()
    };
    Ok(if pronta { Semaforo::Giallo } else { Semaforo::Rosso })
}

// ---------------------------------------------------------------------------
// Phases (RefreshFasi + Fase control)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatoFase {
    DaFare,
    InCorso,
    Sospesa,
    Terminata,
}

impl StatoFase {
    pub fn nome(self) -> &'static str {
        match self {
            StatoFase::DaFare => "Da fare",
            StatoFase::InCorso => "In corso",
            StatoFase::Sospesa => "Sospesa",
            StatoFase::Terminata => "Terminata",
        }
    }
}

/// ESP_FormMain::RefreshFasi (0x1e750): PD_DTEFIN → Terminata; PD_DTEINI without PD_DTESOSP
/// → InCorso; anything else → DaFare. A started-then-suspended phase is DaFare for the
/// original; here it keeps the label "Sospesa" but every rule treats it as DaFare
/// (neither InCorso nor Terminata).
pub fn stato_fase(iniziata: bool, sospesa: bool, finita: bool) -> StatoFase {
    if finita {
        StatoFase::Terminata
    } else if iniziata && !sospesa {
        StatoFase::InCorso
    } else if iniziata {
        StatoFase::Sospesa
    } else {
        StatoFase::DaFare
    }
}

#[derive(Clone, Debug)]
pub struct Fase {
    pub progr: i64,
    pub descr: String,
    pub cod_fase: String,
    pub tipo: String,
    pub stato: StatoFase,
    pub ora_inizio: Option<NaiveDateTime>,
    pub pezzi_prodotti: f64,
    pub pezzi_mancanti: f64,
    pub durata_teorica: Option<f64>,
    pub durata_prev: Option<f64>,
    pub semaforo: Semaforo,
}

impl Fase {
    pub fn produzione(&self) -> bool {
        matches!(self.tipo.trim(), "2" | "4")
    }
}

pub async fn fasi(db: &Db, az: &str, setup: &Setup, amb: &Ambiente, mac: &str, ins_parziale: bool) -> Result<Vec<Fase>, String> {
    if amb.anno.is_empty() {
        return Ok(Vec::new());
    }
    let n: f64 = amb.num.parse().unwrap_or(0.0);
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(amb.anno.as_str());
    let p_n = s.arg(Param::Float(n));
    let p_t = s.arg(amb.tipo.as_str());
    let p_m = s.arg(mac);
    let p_vf = s.arg(setup.vf.as_str());
    let p_vs = s.arg(setup.vs.as_str());
    let p_ore = s.arg(setup.ore.as_str());
    let nascondi = if setup.nascondi_fasi_terminate { " AND PD_DTEFIN IS NULL" } else { "" };
    s.text = format!(
        "SELECT PD_PROGR, PD_FASI.PD_DESFAS, PD_QTAORA, PD_DTEINI, PD_OREINIZIO, PD_ADDETTI, PD_DTEFIN, PD_FASI.PD_CODFAS, PD_DTESOSP, PD_TIPIFASI.PD_TYP,
           (SELECT SUM(D.PD_QUANTI) FROM PD_COMM_D D WHERE D.PD_ANNOCOM = {p_a} AND D.PD_NUMCOM = {p_n} AND D.PD_TIPOCOM = {p_t} AND D.PD_IDAZIENDA = {p_az}) AS DA_PROD,
           (SELECT SUM(X.PD_NPEZZI) FROM PD_DATIPROD X WHERE X.PD_EXTRACICLO IS NULL AND (X.PD_CODOPE = {p_vf} OR X.PD_CODOPE = {p_vs}) AND X.PD_ANNOCOMRAG = {p_a} AND X.PD_NUMCOMRAG = {p_n}
              AND X.PD_TIPOCOM = 'S' AND X.PD_IDAZIENDA = {p_az} AND X.PD_FASE = PD_FASI.PD_PROGR) AS PEZZI_TOT,
           (SELECT SUM(X.PD_NPEZZI) FROM PD_DATIPROD X WHERE X.PD_EXTRACICLO IS NULL AND (X.PD_CODOPE = {p_vf} OR X.PD_CODOPE = {p_vs}) AND X.PD_ANNOCOMRAG = {p_a} AND X.PD_NUMCOMRAG = {p_n}
              AND X.PD_TIPOCOM = 'S' AND X.PD_IDAZIENDA = {p_az} AND X.PD_FASE = PD_FASI.PD_PROGR AND X.PD_MACCHINA = {p_m}) AS PEZZI_MAC,
           (SELECT SUM(X.PD_TOTSEC) FROM PD_DATIPROD X WHERE X.PD_EXTRACICLO IS NULL AND X.PD_CODOPE = {p_ore} AND X.PD_FASE = PD_FASI.PD_PROGR AND X.PD_ANNOCOM = {p_a}
              AND X.PD_COMMESSA = {p_n} AND X.PD_TIPOCOM = {p_t} AND X.PD_MACCHINA = {p_m} AND X.PD_IDAZIENDA = {p_az}) AS DURATA_REG
         FROM PD_FASI, PD_TIPIFASI
         WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_CODMAC = {p_m} AND PD_FASI.PD_IDAZIENDA = {p_az}
           AND PD_TYP != 5 AND PD_FASI.PD_CODFAS = PD_TIPIFASI.PD_CODFAS AND PD_FASI.PD_IDAZIENDA = PD_TIPIFASI.PD_IDAZIENDA{nascondi}
         ORDER BY PD_PROGR"
    );
    let t = db.query(s).await?;
    let now = Local::now().naive_local();
    let mut out = Vec::new();
    for r in &t.rows {
        let progr = cell_i(t.get(r, "PD_PROGR"));
        let ini = cell_date(t.get(r, "PD_DTEINI"));
        let ora = cell_time(t.get(r, "PD_OREINIZIO"));
        let ora_inizio = match (ini, ora) {
            (Some(d), Some(o)) => Some(d.and_time(o)),
            _ => None,
        };
        let iniziata = ini.is_some();
        let sospesa = !t.get(r, "PD_DTESOSP").is_null();
        let finita = !t.get(r, "PD_DTEFIN").is_null();
        let stato = stato_fase(iniziata, sospesa, finita);
        let tipo = t.s(r, "PD_TYP");
        let num = cell_f(t.get(r, "DA_PROD"));
        let num6 = cell_f(t.get(r, "PEZZI_TOT"));
        let num7 = cell_f(t.get(r, "PEZZI_MAC"));
        let qta_ora = cell_f(t.get(r, "PD_QTAORA"));
        let produzione = matches!(tipo.trim(), "2" | "4");
        let durata_teorica = if qta_ora > 0.0 {
            Some(if produzione { 3600.0 * num / qta_ora } else { 3600.0 / qta_ora })
        } else {
            None
        };
        let mancanti = if ins_parziale { (num - num6).max(0.0) } else { num - num6 };
        let mut durata_prev = None;
        if matches!(stato, StatoFase::InCorso | StatoFase::Terminata) {
            let mut reg = cell_f(t.get(r, "DURATA_REG"));
            if stato == StatoFase::InCorso {
                if let Some(oi) = ora_inizio {
                    reg += (now - oi).num_seconds() as f64;
                }
            }
            if produzione {
                if num7 > 0.0 {
                    durata_prev = Some(reg / num7 * (num - num6));
                }
            } else if let Some(dt) = durata_teorica {
                durata_prev = Some(dt - reg);
            }
        }
        let semaforo = colore_fase(db, az, setup, &amb.anno, &amb.num, &amb.tipo, progr, mac, num).await?;
        out.push(Fase {
            progr,
            descr: t.s(r, "PD_DESFAS"),
            cod_fase: t.s(r, "PD_CODFAS"),
            tipo,
            stato,
            ora_inizio,
            pezzi_prodotti: num7,
            pezzi_mancanti: mancanti,
            durata_teorica,
            durata_prev,
            semaforo,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// VERSAMENTI: what was recorded in one day on the workstation's machines (reads only)

#[derive(Clone, Debug)]
pub struct VersamentoGiorno {
    pub quando: Option<NaiveDateTime>,
    pub macchina: String,
    pub cod_dip: String,
    pub operatore: String,
    pub commessa: String,
    pub gruppo: String,
    pub fase: i64,
    pub fase_descr: String,
    pub operazione: String,
    pub codart: String,
    pub descr: String,
    pub qta: f64,
    pub um: String,
    pub ndec: usize,
}

/// PD_DATIPROD rows of the day with the output operations (VF on the last production
/// phase, VS on the others: RaccoltaDatiHelper.GetCodOperazioneVersamenti 0x4a624),
/// single orders, on these machines; newest first. Same base and filters as the original's
/// list: RD.Dal.MSSql DatoProduzioneDal::ListByFilter (0x7e40) — `PD_IDAZIENDA`, `PD_CODOPE IS
/// NULL OR <> 'SPC'`, AggiungiFiltri (0x7f78) `PD_DATA >= DataDa`, `PD_MACCHINA`, `PD_TIPOCOM`,
/// operations; no filter on cancelled or extra-cycle rows; ordered by PD_DATA, PD_ORAINIZ,
/// PD_PROGR (here reversed).
pub async fn versamenti_giorno(db: &Db, az: &str, setup: &Setup, giorno: NaiveDate, macchine: &[String]) -> Result<Vec<VersamentoGiorno>, String> {
    if macchine.is_empty() {
        return Ok(Vec::new());
    }
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_vf = s.arg(setup.vf.as_str());
    let p_vs = s.arg(setup.vs.as_str());
    let p_da = s.arg(Param::Date(giorno));
    let p_a = s.arg(Param::Date(giorno.succ_opt().unwrap_or(giorno)));
    let in_mac = macchine.iter().map(|m| s.arg(m.as_str())).collect::<Vec<_>>().join(",");
    s.text = format!(
        "SELECT TOP 2000 P.PD_ORAINIZ, P.PD_DATA, P.PD_MACCHINA, P.PD_CODDIP, U.NOME, P.PD_ANNOCOM, P.PD_COMMESSA, P.PD_ANNOCOMRAG, P.PD_NUMCOMRAG,
           P.PD_FASE, P.PD_CODOPE, P.PD_CODART, P.PD_NPEZZI, A.MG_DESC, A.MG_UNMIS, A.DECIMALIQTA,
           (SELECT TOP 1 F.PD_DESFAS FROM PD_FASI F WHERE F.PD_IDAZIENDA = P.PD_IDAZIENDA AND F.PD_ANNOCOM = P.PD_ANNOCOMRAG
              AND F.PD_NUMCOM = P.PD_NUMCOMRAG AND F.PD_TIPOCOM = 'R' AND F.PD_PROGR = P.PD_FASE) AS FASE_DESCR
         FROM PD_DATIPROD P
         LEFT JOIN Users U ON U.USERNAME = P.PD_CODDIP AND U.IDAZIENDA = P.PD_IDAZIENDA
         LEFT JOIN MG_ARTICOLI A ON A.MG_CODI = P.PD_CODART AND A.MG_IDAZIENDA = P.PD_IDAZIENDA
         WHERE P.PD_IDAZIENDA = {p_az} AND (P.PD_CODOPE IS NULL OR P.PD_CODOPE <> 'SPC')
           AND (P.PD_CODOPE = {p_vf} OR P.PD_CODOPE = {p_vs}) AND P.PD_TIPOCOM = 'S'
           AND P.PD_DATA >= {p_da} AND P.PD_DATA < {p_a} AND P.PD_MACCHINA IN ({in_mac})
         ORDER BY P.PD_DATA DESC, P.PD_ORAINIZ DESC, P.PD_PROGR DESC"
    );
    let t = db.query(s).await?;
    Ok(t.rows
        .iter()
        .map(|r| {
            let ndec = t.get(r, "DECIMALIQTA").i64().map(|d| d as usize).unwrap_or(setup.ndec);
            let quando = match (t.get(r, "PD_ORAINIZ"), t.get(r, "PD_DATA")) {
                (Cell::DateTime(d), _) | (_, Cell::DateTime(d)) => Some(*d),
                _ => None,
            };
            VersamentoGiorno {
                quando,
                macchina: t.s(r, "PD_MACCHINA").trim().to_string(),
                cod_dip: t.s(r, "PD_CODDIP").trim().to_string(),
                operatore: t.s(r, "NOME").trim().to_string(),
                commessa: format!("{}-{}", t.s(r, "PD_ANNOCOM"), t.s(r, "PD_COMMESSA")),
                gruppo: format!("{}-{}", t.s(r, "PD_ANNOCOMRAG"), t.s(r, "PD_NUMCOMRAG")),
                fase: cell_i(t.get(r, "PD_FASE")),
                fase_descr: t.s(r, "FASE_DESCR").trim().to_string(),
                operazione: t.s(r, "PD_CODOPE").trim().to_string(),
                codart: t.s(r, "PD_CODART").trim().to_string(),
                descr: t.s(r, "MG_DESC").trim().to_string(),
                qta: cell_f(t.get(r, "PD_NPEZZI")),
                um: t.s(r, "MG_UNMIS").trim().to_string(),
                ndec,
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Recorded quantities (RefreshPezzi)

#[derive(Clone, Debug)]
pub struct Versato {
    pub num_reg: String,
    pub articolo: String,
    pub commessa: String,
    pub qta: String,
    pub um: String,
    pub quando: Option<NaiveDateTime>,
    pub operatore: String,
}

pub async fn versati(db: &Db, az: &str, setup: &Setup, amb: &Ambiente, mac: &str) -> Result<(bool, Vec<Versato>), String> {
    if amb.num.is_empty() || amb.progr.is_empty() {
        return Ok((false, Vec::new()));
    }
    let n: f64 = amb.num.parse().unwrap_or(0.0);
    let p: f64 = amb.progr.parse().unwrap_or(0.0);
    // FaseFunz.IsLastLavFase: no later production phase (types 2/4) in the order
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(amb.anno.as_str());
    let p_n = s.arg(Param::Float(n));
    let p_t = s.arg(amb.tipo.as_str());
    let p_p = s.arg(Param::Float(p));
    s.text = format!(
        "SELECT COUNT(*) AS N FROM PD_FASI INNER JOIN PD_TIPIFASI ON (PD_FASI.PD_IDAZIENDA = PD_TIPIFASI.PD_IDAZIENDA AND PD_FASI.PD_CODFAS = PD_TIPIFASI.PD_CODFAS)
         WHERE PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t} AND PD_FASI.PD_IDAZIENDA = {p_az} AND PD_FASI.PD_PROGR > {p_p} AND (PD_TYP = '2' OR PD_TYP = '4')"
    );
    let t = db.query(s).await?;
    let ultima = t.rows.first().map(|r| cell_i(t.get(r, "N")) == 0).unwrap_or(true);
    let ope = if ultima { setup.vf.clone() } else { setup.vs.clone() };

    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(amb.anno.as_str());
    let p_n = s.arg(Param::Float(n));
    let p_m = s.arg(mac);
    let p_p = s.arg(Param::Float(p));
    let p_o = s.arg(ope.as_str());
    s.text = format!(
        "SELECT P.PD_PROGR, P.PD_CODART, P.PD_ANNOCOM, P.PD_COMMESSA, P.PD_NPEZZI, P.PD_ORAINIZ, U.NOME, A.MG_UNMIS, A.DECIMALIQTA
         FROM PD_DATIPROD P
         LEFT JOIN Users U ON U.USERNAME = P.PD_CODDIP AND U.IDAZIENDA = P.PD_IDAZIENDA
         LEFT JOIN MG_ARTICOLI A ON A.MG_CODI = P.PD_CODART AND A.MG_IDAZIENDA = P.PD_IDAZIENDA
         WHERE P.PD_IDAZIENDA = {p_az} AND (P.PD_CODOPE IS NULL OR P.PD_CODOPE <> 'SPC') AND P.PD_MACCHINA = {p_m} AND P.PD_TIPOCOM = 'S'
           AND P.PD_ANNOCOMRAG = {p_a} AND P.PD_NUMCOMRAG = {p_n} AND P.PD_FASE = {p_p} AND P.PD_CODOPE = {p_o}
         ORDER BY P.PD_DATA, P.PD_ORAINIZ, P.PD_PROGR"
    );
    let t = db.query(s).await?;
    let rows = t
        .rows
        .iter()
        .map(|r| {
            let nd = t.get(r, "DECIMALIQTA").i64().map(|d| d as usize).unwrap_or(setup.ndec);
            Versato {
                num_reg: t.s(r, "PD_PROGR"),
                articolo: t.s(r, "PD_CODART"),
                commessa: format!("{}-{}", t.s(r, "PD_ANNOCOM"), t.s(r, "PD_COMMESSA")),
                qta: fmt_qta(cell_f(t.get(r, "PD_NPEZZI")), nd),
                um: t.s(r, "MG_UNMIS"),
                quando: match t.get(r, "PD_ORAINIZ") {
                    Cell::DateTime(d) => Some(*d),
                    _ => None,
                },
                operatore: t.s(r, "NOME"),
            }
        })
        .collect();
    Ok((ultima, rows))
}

// ---------------------------------------------------------------------------
// Items to produce (CaricaArticoliVers_Prel, "Articoli da produrre")

#[derive(Clone, Debug)]
pub struct RigaArticolo {
    pub commessa: String,
    pub codart: String,
    pub descr: String,
    pub qta: f64,
    pub um: String,
    pub rif_cliente: String,
}

pub async fn articoli_commessa(db: &Db, az: &str, amb: &Ambiente) -> Result<Vec<RigaArticolo>, String> {
    if amb.num.is_empty() {
        return Ok(Vec::new());
    }
    let n: f64 = amb.num.parse().unwrap_or(0.0);
    let mut s = Sql::new("");
    let p_az = s.arg(az);
    let p_a = s.arg(amb.anno.as_str());
    let p_n = s.arg(Param::Float(n));
    let p_t = s.arg(amb.tipo.as_str());
    let singole = if amb.tipo == "S" {
        format!("SELECT {p_a} AS PD_ANNOCOMPRV, {p_n} AS PD_NUMCOMPRV")
    } else {
        format!("SELECT DISTINCT PD_ANNOCOMPRV, PD_NUMCOMPRV FROM PD_COMM_D WHERE PD_IDAZIENDA = {p_az} AND PD_ANNOCOM = {p_a} AND PD_NUMCOM = {p_n} AND PD_TIPOCOM = {p_t}")
    };
    s.text = format!(
        "WITH SG AS ({singole})
         SELECT D.PD_ANNOCOM, D.PD_NUMCOM, D.PD_CODART, SUM(D.PD_QUANTI) AS QTA, D.PD_ANNOORDCLI, D.PD_ORDINECLI, D.PD_RIFERIMENTOCLI,
                A.MG_DESC, A.MG_UNMIS, A.MG_RIGALIBERA, T.PD_NOTE
         FROM SG
         INNER JOIN PD_COMM_D D ON D.PD_IDAZIENDA = {p_az} AND D.PD_ANNOCOM = SG.PD_ANNOCOMPRV AND D.PD_NUMCOM = SG.PD_NUMCOMPRV AND D.PD_TIPOCOM = 'S'
         LEFT JOIN MG_ARTICOLI A ON A.MG_CODI = D.PD_CODART AND A.MG_IDAZIENDA = D.PD_IDAZIENDA
         LEFT JOIN PD_COMM_T T ON T.PD_IDAZIENDA = D.PD_IDAZIENDA AND T.PD_ANNOCOM = D.PD_ANNOCOM AND T.PD_NUMCOM = D.PD_NUMCOM AND T.PD_TIPOCOM = D.PD_TIPOCOM
         GROUP BY D.PD_ANNOCOM, D.PD_NUMCOM, D.PD_CODART, D.PD_ANNOORDCLI, D.PD_ORDINECLI, D.PD_RIFERIMENTOCLI, A.MG_DESC, A.MG_UNMIS, A.MG_RIGALIBERA, T.PD_NOTE
         ORDER BY D.PD_ANNOCOM, D.PD_NUMCOM"
    );
    let t = db.query(s).await?;
    Ok(t.rows
        .iter()
        .map(|r| {
            let art = Articolo { descr: t.s(r, "MG_DESC"), um: t.s(r, "MG_UNMIS"), riga_libera: t.s(r, "MG_RIGALIBERA") == "Y", decimali: None };
            RigaArticolo {
                commessa: format!("{}-{}", t.s(r, "PD_ANNOCOM"), t.s(r, "PD_NUMCOM")),
                codart: t.s(r, "PD_CODART"),
                descr: art.descr_commessa(&t.s(r, "PD_NOTE")),
                qta: cell_f(t.get(r, "QTA")),
                um: art.um.clone(),
                rif_cliente: format!("{} - {} - {}", t.s(r, "PD_ANNOORDCLI"), t.s(r, "PD_ORDINECLI"), t.s(r, "PD_RIFERIMENTOCLI")),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Companies (ESP_FormLogin, combo "Aziende")

pub async fn aziende(db: &Db) -> Result<Vec<(String, String)>, String> {
    let t = db.query(Sql::new("SELECT AZ_ID, AZ_RAGSOC FROM Aziende WHERE ISNULL(AZ_CANCELLATO, 'N') <> 'S' ORDER BY AZ_ID")).await?;
    Ok(t.rows.iter().map(|r| (t.s(r, "AZ_ID").trim().to_string(), t.s(r, "AZ_RAGSOC").trim().to_string())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_state_as_refresh_fasi() {
        // (started, suspended, finished)
        assert_eq!(stato_fase(true, false, true), StatoFase::Terminata);
        assert_eq!(stato_fase(false, false, true), StatoFase::Terminata);
        assert_eq!(stato_fase(true, false, false), StatoFase::InCorso);
        // the original's DaFare; labelled "Sospesa", never InCorso/Terminata
        assert_eq!(stato_fase(true, true, false), StatoFase::Sospesa);
        assert_eq!(stato_fase(false, false, false), StatoFase::DaFare);
        assert_eq!(stato_fase(false, true, false), StatoFase::DaFare);
    }

    #[test]
    fn scanned_codes() {
        assert_eq!(analizza_codice("105266"), Some((None, "105266".into())));
        assert_eq!(analizza_codice("26-20105266\r\n"), Some((Some("26".into()), "20105266".into())));
        assert_eq!(analizza_codice("2026/20105266"), Some((Some("2026".into()), "20105266".into())));
        assert_eq!(analizza_codice("12"), None);
        assert_eq!(rango("20105266", "20105266"), Some(0));
        assert_eq!(rango("20105266", "105266"), Some(1));
        assert_eq!(rango("20105266", "0526"), Some(2));
        assert_eq!(rango("20105266", "999"), None);
    }

    #[test]
    fn quantities_like_the_original() {
        assert_eq!(fmt_qta(68.0, 3), "68,000");
        assert_eq!(fmt_qta(703.0, 3), "703,000");
        assert_eq!(fmt_qta(12.5, 0), "12");
        assert_eq!(fmt_durata(3720.0), "1:02");
    }

    fn c(sem: Semaforo, lancio: i64, prio: i64, d: u32, num: &str) -> Commessa {
        Commessa {
            anno: "2026".into(),
            num: num.into(),
            tipo: "R".into(),
            annotazione: String::new(),
            note: String::new(),
            datcom: NaiveDate::from_ymd_opt(2026, 1, d),
            dt_consegna: None,
            priorita: prio,
            ord_lancio: lancio,
            codart: String::new(),
            descr_art: String::new(),
            um: String::new(),
            qta: 0.0,
            prodotti: 0.0,
            prodotti_tot: 0.0,
            scarti: 0.0,
            semaforo: sem,
        }
    }

    #[test]
    fn order_sort_matches_refresh_commesse() {
        let mut v = vec![
            c(Semaforo::Giallo, 5, 999, 3, "3"),
            c(Semaforo::Verde, 9, 999, 9, "9"),
            c(Semaforo::Giallo, 5, 1, 9, "2"),
            c(Semaforo::Rosso, 1, 1, 1, "1"),
        ];
        ordina_commesse(&mut v, "P");
        let nums: Vec<&str> = v.iter().map(|c| c.num.as_str()).collect();
        assert_eq!(nums, ["9", "2", "3", "1"]);
    }
}
