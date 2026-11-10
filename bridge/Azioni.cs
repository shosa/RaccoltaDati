// The original's actions, step by step in the original's order (ESP_FormMain.cs).
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Esprit.Logic;
using Esprit.Sys.Dal;
using Microsoft.Data.SqlClient;
using RD.Dal.MSSql;
using RD.Entities;
using RD.Logic;

// What the original keeps in AppState for the selected machine
// (RefreshMacchine + RefreshAmbiente).
class Mac
{
    public string Cod = "";
    public string Reparto;
    public string Famiglia = "";
    public int IdMacchina;
    public bool AvanzaFasi;
    public bool VersaSoloPezziBuoni;
    public string SwInterfaceType;
    public int? MsgAttrezzaggio;
    public int? MsgLavorazione;
    public string CodDip = "";
    public string AnnoComm = "";
    public string NumComm = "";
    public string TipoComm = "";
    public string ProgrFase = "";
    public string CodFase = "";
    public int IdOperatore;
    public int? IdGruppo;

    public int? NAnno { get { return AnnoComm.Trim() == "" ? (int?)null : Convert.ToInt32(AnnoComm); } }
    public int? NNum { get { return NumComm.Trim() == "" ? (int?)null : Convert.ToInt32(NumComm); } }
    public int? NProgr { get { return ProgrFase.Trim() == "" ? (int?)null : Convert.ToInt32(ProgrFase); } }
    public AnnoNumeroTipoComme Commessa
    {
        get { return NAnno.HasValue && NNum.HasValue ? new AnnoNumeroTipoComme(NAnno.Value, NNum.Value, TipoComm) : null; }
    }
    public FaseKey FaseKey
    {
        get
        {
            return NNum.HasValue && NProgr.HasValue
                ? new FaseKey { Commessa = Commessa, Progressivo = NProgr.Value, CodMacchina = Cod }
                : null;
        }
    }
}

// One phase row as the original's Fase control holds it (RefreshFasi).
class FaseInfo
{
    public int Progr;
    public string Descr = "";
    public string CodFase = "";
    public string TipoFase;
    public decimal NumAddetti = 1m;
    public DateTime OraInizio = DateTime.MinValue;
    public bool Iniziata, Sospesa, Finita;
    public int? IdTemplate, IdSessione;
    public bool InCorso { get { return Finita ? false : (Iniziata && !Sospesa); } }
    public int TipoFaseInt { get { int t; return int.TryParse((TipoFase ?? "").Trim(), out t) ? t : 0; } }
}

static partial class Ponte
{
    public static string Az = "";
    public static RD.Logic.Settings S;
    static bool UsaBadge;
    static bool TornaScheda;
    static string[] OperatoriAmmessi = new[] { "" };

    static Dictionary<string, string> Dispatch(Req r)
    {
        switch (r.Cmd)
        {
            case "init": return Init(r);
            case "ping": return Ok();
            case "testo": { var d = Ok(); d["testo"] = Testo(r.S("code")); return d; }
            case "operatore": return Operatore(r);
            case "turno_inizia": return TurnoInizia(r);
            case "turno_fine": return TurnoFine(r);
            case "macchina_seleziona": return MacchinaSeleziona(r);
            case "macchina_controlla": return MacchinaControlla(r);
            case "commessa_inizia": return CommessaInizia(r);
            case "commessa_sospendi": return CommessaSospendi(r);
            case "fase_inizia": return FaseInizia(r);
            case "fase_utilizza": return FaseUtilizza(r);
            case "fase_chiudi": return FaseChiudi(r);
            case "vers_carica": return VersCarica(r);
            case "vers_conferma": return VersConferma(r);
        }
        throw new UserError("Comando sconosciuto: " + r.Cmd);
    }

    // Program.Main + ESP_FormLogin.B_Conferma_Click
    static Dictionary<string, string> Init(Req r)
    {
        Az = r.S("azienda");
        WinDevHelper.Inizializza(r.S("server"), r.S("database"), r.S("user"), r.S("password"), Az);
        SQLServer.idAzienda = Az;
        Configurazioni.IdAzienda = Az;
        Configurazioni.Database = r.S("database");
        S = new RD.Logic.Settings();
        UsaBadge = r.S("usa_badge") == "Y";
        TornaScheda = r.S("torna_scheda") == "Y";
        OperatoriAmmessi = r.S("operatori").Split(',');
        var d = Ok();
        d["versione"] = "1";
        return d;
    }

    static DBContext Db(Mac m) { return DBContext.GetNewDbContext(Az, m == null ? 0 : m.IdOperatore); }
    static DBContext DbT(Mac m) { return DBContext.GetNewInTransactionDbContext(Az, m == null ? 0 : m.IdOperatore); }

    static string Str(object o) { return o == null || o == DBNull.Value ? "" : o.ToString(); }

    static Mac LoadMac(string cod)
    {
        var m = new Mac { Cod = cod };
        using (SqlConnection con = SQLServer.getNewConn())
        {
            using (var cmd = new SqlCommand("SELECT DESCR, IDREPCDC, CE_FAMIGLIA, CE_AVANZAFASI, ID, CE_SILOS, CE_SWINTERFACETYPE, CE_NUMMESSATTR, CE_NUMMESSPROD, CE_VERSA_SOLO_BUONI FROM CELPRO WHERE CODICE = @C AND CE_IDAZIENDA = @A", con))
            {
                cmd.Parameters.AddWithValue("@C", cod);
                cmd.Parameters.AddWithValue("@A", Az);
                using (var dr = cmd.ExecuteReader())
                {
                    if (!dr.Read()) throw new UserError("Macchina " + cod + " non trovata");
                    int idRep = Convert.ToInt32(dr["IDREPCDC"]);
                    if (idRep > 0) m.Reparto = Funzioni.GetCodReparto(idRep);
                    m.Famiglia = dr["CE_FAMIGLIA"] == DBNull.Value ? "" : dr["CE_FAMIGLIA"].ToString().Trim();
                    m.AvanzaFasi = Str(dr["CE_AVANZAFASI"]) == "Y";
                    m.IdMacchina = Convert.ToInt32(dr["ID"]);
                    m.SwInterfaceType = dr["CE_SWINTERFACETYPE"] == DBNull.Value ? null : dr["CE_SWINTERFACETYPE"].ToString().TrimEnd();
                    m.MsgAttrezzaggio = dr["CE_NUMMESSATTR"] == DBNull.Value ? (int?)null : Convert.ToInt32(dr["CE_NUMMESSATTR"]);
                    m.MsgLavorazione = dr["CE_NUMMESSPROD"] == DBNull.Value ? (int?)null : Convert.ToInt32(dr["CE_NUMMESSPROD"]);
                    m.VersaSoloPezziBuoni = Str(dr["CE_VERSA_SOLO_BUONI"]) == "Y";
                }
            }
            using (var cmd = new SqlCommand(@"SELECT R.RD_CODDIP, R.RD_ANNOCOM, R.RD_NUMCOM, R.RD_TIPOCOM, R.RD_PROGRFASE, F.PD_CODFAS
FROM RD_MACCHINE R LEFT OUTER JOIN PD_FASI F ON (F.PD_ANNOCOM = R.RD_ANNOCOM AND F.PD_IDAZIENDA = R.RD_IDAZIENDA AND F.PD_NUMCOM = R.RD_NUMCOM
 AND F.PD_TIPOCOM = R.RD_TIPOCOM AND R.RD_PROGRFASE = F.PD_PROGR AND R.RD_CODMAC = F.PD_CODMAC)
WHERE R.RD_CODMAC = @C AND R.RD_IDAZIENDA = @A", con))
            {
                cmd.Parameters.AddWithValue("@C", cod);
                cmd.Parameters.AddWithValue("@A", Az);
                using (var dr = cmd.ExecuteReader())
                {
                    if (dr.Read())
                    {
                        m.CodDip = Str(dr["RD_CODDIP"]).TrimEnd();
                        m.AnnoComm = Str(dr["RD_ANNOCOM"]).TrimEnd();
                        m.NumComm = Str(dr["RD_NUMCOM"]).TrimEnd();
                        m.TipoComm = Str(dr["RD_TIPOCOM"]).TrimEnd();
                        m.ProgrFase = Str(dr["RD_PROGRFASE"]).TrimEnd();
                        m.CodFase = Str(dr["PD_CODFAS"]).TrimEnd();
                    }
                }
            }
        }
        // decimal columns come back as "601705911" or "60": keep integers only
        m.NumComm = Intero(m.NumComm);
        m.ProgrFase = Intero(m.ProgrFase);
        if (m.CodDip != "")
        {
            Esprit.Entities.User u = new UserBiz().GetUser(m.CodDip);
            if (u != null)
            {
                m.IdOperatore = u.Id;
                m.IdGruppo = u.IdGruppo <= 0 ? (int?)null : u.IdGruppo;
            }
        }
        return m;
    }

    static string Intero(string s)
    {
        decimal d;
        if (s == "" || !decimal.TryParse(s, NumberStyles.Any, CultureInfo.CurrentCulture, out d)) return s;
        return decimal.Truncate(d).ToString(CultureInfo.InvariantCulture);
    }

    static FaseInfo LoadFase(Mac m, int progr)
    {
        using (SqlConnection con = SQLServer.getNewConn())
        using (var cmd = new SqlCommand(@"SELECT PD_PROGR, PD_FASI.PD_DESFAS, PD_DTEINI, PD_OREINIZIO, PD_ADDETTI, PD_DTEFIN, PD_FASI.PD_CODFAS, PD_DTESOSP, PD_TE_ID, PD_TIPIFASI.PD_TYP, PD_IDSESSIONE
FROM PD_FASI, PD_TIPIFASI WHERE PD_ANNOCOM = @AN AND PD_NUMCOM = @NU AND PD_TIPOCOM = @TI AND PD_CODMAC = @MA AND PD_FASI.PD_IDAZIENDA = @AZ
 AND PD_TYP != 5 AND PD_FASI.PD_CODFAS = PD_TIPIFASI.PD_CODFAS AND PD_FASI.PD_IDAZIENDA = PD_TIPIFASI.PD_IDAZIENDA AND PD_PROGR = @PR", con))
        {
            cmd.Parameters.AddWithValue("@AN", m.AnnoComm);
            cmd.Parameters.AddWithValue("@NU", m.NNum.Value);
            cmd.Parameters.AddWithValue("@TI", m.TipoComm);
            cmd.Parameters.AddWithValue("@MA", m.Cod);
            cmd.Parameters.AddWithValue("@AZ", Az);
            cmd.Parameters.AddWithValue("@PR", progr);
            using (var dr = cmd.ExecuteReader())
            {
                if (!dr.Read()) throw new UserError(Testo(179));
                var f = new FaseInfo { Progr = progr };
                f.Descr = Str(dr["PD_DESFAS"]);
                f.CodFase = Str(dr["PD_CODFAS"]);
                f.TipoFase = dr["PD_TYP"] == DBNull.Value ? null : dr["PD_TYP"].ToString();
                f.NumAddetti = dr["PD_ADDETTI"] == DBNull.Value ? 1m : Convert.ToDecimal(dr["PD_ADDETTI"]);
                if (dr["PD_DTEINI"] != DBNull.Value && dr["PD_OREINIZIO"] != DBNull.Value)
                    f.OraInizio = Convert.ToDateTime(dr["PD_DTEINI"]) + (TimeSpan)dr["PD_OREINIZIO"];
                f.Iniziata = dr["PD_DTEINI"] != DBNull.Value;
                f.Sospesa = dr["PD_DTESOSP"] != DBNull.Value;
                f.Finita = dr["PD_DTEFIN"] != DBNull.Value;
                f.IdTemplate = dr["PD_TE_ID"] == DBNull.Value ? (int?)null : Convert.ToInt32(dr["PD_TE_ID"]);
                f.IdSessione = dr["PD_IDSESSIONE"] == DBNull.Value ? (int?)null : Convert.ToInt32(dr["PD_IDSESSIONE"]);
                return f;
            }
        }
    }

    // ---------------------------------------------------------------- operators

    class TurnoAperto { public string Cod; public DateTime Inizio; }

    // Operatore control: DataIni + OraIni written as "HH:mm" (seconds dropped).
    static List<TurnoAperto> TurniAperti()
    {
        var list = new List<TurnoAperto>();
        using (SqlConnection con = SQLServer.getNewConn())
        using (var cmd = new SqlCommand("SELECT USERNAME, RD_ORAINI, RD_DATAINI FROM RD_TURNI, Users WHERE RD_OPERATORE = USERNAME AND RD_ORAFIN IS NULL AND RD_IDAZIENDA = @A", con))
        {
            cmd.Parameters.AddWithValue("@A", Az);
            using (var dr = cmd.ExecuteReader())
            {
                while (dr.Read())
                {
                    TimeSpan t = (TimeSpan)dr["RD_ORAINI"];
                    DateTime d = Convert.ToDateTime(dr["RD_DATAINI"]).Date;
                    list.Add(new TurnoAperto { Cod = dr["USERNAME"].ToString(), Inizio = d.Add(new TimeSpan(t.Hours, t.Minutes, 0)) });
                }
            }
        }
        return list;
    }

    // RaccoltaDatiHelper.SelezioneOperatore (code + optional password).
    static Dictionary<string, string> Operatore(Req r)
    {
        string cod = r.S("cod");
        if (cod == "") throw new UserError("Devi inserire un codice");
        if (!(OperatoriAmmessi[0] == "") && !OperatoriAmmessi.Contains(cod)) throw new UserError("UTENTE NON TROVATO");
        var userBiz = new UserBiz();
        Esprit.Entities.User user = userBiz.GetUser(cod);
        if (user == null || string.IsNullOrWhiteSpace(user.UserName)) throw new UserError("Codice operatore associato a nessun operatore");
        bool turno;
        using (SqlConnection con = SQLServer.getNewConn()) turno = Turno.CheckTurnoOperatoreInCorso(cod, con);
        var d = Ok();
        d["nome"] = (user.Nome ?? "").Trim();
        d["turno_aperto"] = turno ? "Y" : "N";
        if (!UsaBadge && !userBiz.CheckPasswordUser(user, ""))
        {
            if (!r.Args.ContainsKey("pwd")) { d["serve_password"] = "Y"; return d; }
            if (r.S("pwd") == "") throw new UserError("Devi inserire una password");
            if (!userBiz.CheckPasswordUser(user, r.S("pwd"))) throw new UserError("Password errata");
        }
        d["ok"] = "Y";
        return d;
    }

    // Inizia_Turno_Click -> IniziaTurno
    static Dictionary<string, string> TurnoInizia(Req r)
    {
        string cod = r.S("cod");
        using (SqlConnection con = SQLServer.getNewConn())
        {
            // The original shows "Operatore già collegato" and still opens a second shift;
            // here the duplicate is not written.
            if (Turno.CheckTurnoOperatoreInCorso(cod, con)) throw new UserError("Operatore già collegato");
        }
        using (DBContext db = DBContext.GetNewDbContext(Az, 0))
        {
            new TurnoDal(db).InizioTurno(cod, DateTime.Now);
        }
        return Ok();
    }

    static void StatiOperatore(string codDip, bool soloFasiAperte, out List<StatoRD> stati, out List<DatoProduzione> interventi)
    {
        using (DBContext db = DBContext.GetNewDbContext(Az, 0))
        {
            List<StatoRD> source = new StatoRDDal(db).List();
            stati = soloFasiAperte
                ? source.Where(s => s.CodDipendente == codDip && s.ProgrFase.HasValue).ToList()
                : source.Where(s => s.CodDipendente == codDip).ToList();
            interventi = new GestoreInterventiAggiuntivi(db).GetInterventiAggiuntiviApertiOperatore(codDip);
        }
    }

    // Finisci_Turno_Click -> FineTurnoOperatoreSelezionato
    static Dictionary<string, string> TurnoFine(Req r)
    {
        string text = r.S("cod").TrimEnd();
        using (SqlConnection con = SQLServer.getNewConn())
        {
            if (!Turno.CheckTurnoOperatoreInCorso(text, con)) throw new UserError(Testo(163));
        }
        List<StatoRD> list; List<DatoProduzione> list2;
        StatiOperatore(text, true, out list, out list2);
        if (S.VERSAMENTO_SCARTO_CAMBIO_TURNO)
            throw new UserError("Versamento e scarto al cambio turno: per ora usare il programma originale");
        TurnoAperto turno = TurniAperti().FirstOrDefault(t => t.Cod.TrimEnd() == text);
        if (turno == null) throw new UserError(Testo(163));

        bool fasiSospese = false;
        if (list.Count > 0)
        {
            bool sospendi = S.SOSPENDI_FASI_FINE_TURNO && (S.GESTIONE_AUTO_INTAGG || Si(r, "468"));
            if (!sospendi)
            {
                var sb = new System.Text.StringBuilder();
                sb.AppendLine(Testo(417));
                foreach (StatoRD s in list)
                    sb.AppendLine("Macchina: " + s.CodMacchina + " Commessa: " + s.Commessa.AnnoComme + "-" + s.Commessa.NumComme + " Fase: " + s.ProgrFase);
                throw new UserError(sb.ToString().TrimEnd());
            }
            fasiSospese = true;
        }

        using (DBContext db = DBContext.GetNewInTransactionDbContext(Az, 0))
        {
            DateTime now = DateTime.Now;
            var gia = new GestoreInterventiAggiuntivi(db);
            var list3 = new List<DatoProduzione>();
            if (fasiSospese)
            {
                var gf = new GestoreFasi(S.OPERAZIONE_LAVORO_SU_FASE, db);
                foreach (StatoRD item in list)
                {
                    var fk = new FaseKey { Commessa = item.Commessa, Progressivo = item.ProgrFase.Value, CodMacchina = item.CodMacchina };
                    list3.AddRange(gia.GetInterventiAggiuntiviApertiFase(fk));
                    gf.SospendiFase(fk, now);
                    RDMacchineFunz.ImpostaFaseUtilizzata(item.CodMacchina, "NULL", db.Connection, db.Transaction);
                }
            }
            TimeSpan ts = now.Subtract(turno.Inizio);
            var dp = new DatoProduzione
            {
                DTInizio = turno.Inizio,
                CodOperazione = S.OPERAZIONE_LAVORO_TURNI,
                DTFine = now,
                CodOperatore = text,
                TotSecondi = (int)ts.TotalSeconds
            };
            foreach (DatoProduzione i3 in list2)
                gia.FineInterventoAggiuntivo(i3.Progressivo, i3.AnnoCommeR.Value, i3.NumCommeR.Value, now);
            if (!S.GESTIONE_AUTO_INTAGG)
                foreach (DatoProduzione i4 in list3)
                    gia.FineInterventoAggiuntivo(i4.Progressivo, i4.AnnoCommeR.Value, i4.NumCommeR.Value, now);
            new DatoProduzioneDal(db).Insert(dp);
            new TurnoDal(db).FineTurno(text, now);
            var srd = new StatoRDDal(db);
            srd.TogliOperatoreDaMacchine(text);
            if (S.GESTIONE_AUTO_INTAGG)
            {
                foreach (StatoRD st in list)
                {
                    DatoProduzione dp2 = list3.OrderBy(x => x.DTInizio).FirstOrDefault(x => x.Commessa.Equals(st.Commessa) && x.ProgrFase == st.ProgrFase && x.CodMacchina == st.CodMacchina);
                    if (dp2 == null) continue;
                    gia.FineInterventoAggiuntivo(dp2.Progressivo, dp2.AnnoCommeR.Value, dp2.NumCommeR.Value, now);
                    srd.ImpostaOperatoreSuMacchina(st.CodMacchina, dp2.CodOperatore);
                    Mac mac = LoadMac(st.CodMacchina);
                    Esprit.Entities.User op = new UserBiz().GetUser(st.CodDipendente);
                    new DatoProduzioneDal(db).Insert(new DatoProduzione
                    {
                        CodOperatore = dp2.CodOperatore,
                        DTInizio = now,
                        CodMacchina = st.CodMacchina,
                        CodReparto = mac.Reparto,
                        CodOperazione = S.CAUSALE_CONTROLLO_MACCHINA,
                        IdMacchina = mac.IdMacchina,
                        IdOperatore = op == null ? (int?)null : op.Id,
                        Annullato = "N"
                    });
                    var fk2 = new FaseKey { Commessa = st.Commessa, CodMacchina = st.CodMacchina, Progressivo = st.ProgrFase.Value };
                    new GestoreFasi(S.OPERAZIONE_LAVORO_SU_FASE, db).IniziaFase(fk2, mac.Reparto, dp2.CodOperatore, now);
                    RDMacchineFunz.ImpostaFaseUtilizzata(st.CodMacchina, st.ProgrFase.ToString(), db.Connection, db.Transaction);
                }
            }
            db.CommitTransaction();
        }
        return Ok("124");
    }

    // ---------------------------------------------------------------- machines

    // Macchine_ButtonClick
    static Dictionary<string, string> MacchinaSeleziona(Req r)
    {
        using (DBContext db = DBContext.GetNewInTransactionDbContext(Az, 0))
        {
            new StatoRDDal(db).CreaSeInesistente(r.S("mac"));
            db.CommitTransaction();
        }
        return Ok();
    }

    // bControllaMacchina_Click
    static Dictionary<string, string> MacchinaControlla(Req r)
    {
        string text = r.S("cod").TrimEnd();
        using (SqlConnection con = SQLServer.getNewConn())
        {
            if (!Turno.CheckTurnoOperatoreInCorso(text, con)) throw new UserError(Testo(163));
        }
        if (r.S("mac") == "") throw new UserError(Testo(160));
        Mac m = LoadMac(r.S("mac"));
        if (S.VERSAMENTO_SCARTO_CAMBIO_TURNO)
        {
            List<StatoRD> l1; List<DatoProduzione> l2;
            StatiOperatore(text, false, out l1, out l2);
            if (l1.Count > 0) throw new UserError(Testo(504) + " " + l1[0].CodMacchina);
            if (l2.Count > 0) throw new UserError(Testo(504) + " " + l2[0].CodMacchina);
        }
        if (m.NProgr.HasValue && m.CodDip != "")
        {
            if (m.CodDip == text) return Ok();
            if (S.GESTIONE_AUTO_INTAGG)
            {
                if (string.IsNullOrWhiteSpace(S.CAUSALE_INTAGG_DEFAULT)) throw new UserError(Testo(128) + ": SETUP CAUSALE_INTAGG_DEFAULT");
                using (DBContext db = DbT(m))
                {
                    var gia = new GestoreInterventiAggiuntivi(db);
                    if (gia.CheckInterventoAggiuntivoAperto(text, m.FaseKey)) throw new UserError(Testo(125) + ": " + text);
                    gia.NuovoInterventoAggiuntivo(text, m.FaseKey, S.CAUSALE_INTAGG_DEFAULT);
                    db.CommitTransaction();
                }
                return Ok("124");
            }
            throw new UserError(Testo(161));
        }
        using (DBContext db = DbT(m))
        {
            var srd = new StatoRDDal(db);
            srd.CreaSeInesistente(m.Cod);
            srd.ImpostaOperatoreSuMacchina(m.Cod, text);
            new DatoProduzioneDal(db).Insert(new DatoProduzione
            {
                // As the original: the operator recorded is the one the machine had before
                // (AppState.CodDip is read before the change).
                CodOperatore = m.CodDip,
                DTInizio = DateTime.Now,
                CodMacchina = m.Cod,
                CodReparto = m.Reparto,
                CodOperazione = S.CAUSALE_CONTROLLO_MACCHINA,
                IdMacchina = m.IdMacchina,
                IdOperatore = m.IdOperatore,
                Annullato = "N"
            });
            db.CommitTransaction();
        }
        var d = Ok();
        d["vai_commesse"] = UsaBadge ? "Y" : "N";
        return d;
    }

    // ---------------------------------------------------------------- orders

    // CheckInizioCommessa + IniziaCommessa_Click
    static Dictionary<string, string> CommessaInizia(Req r)
    {
        Mac m = LoadMac(r.S("mac"));
        string anno = r.S("anno"), numero = r.S("num"), tipo = r.S("tipo");
        if (anno == "" || numero == "") throw new UserError(Testo(165));
        if ((m.AnnoComm != "" || m.NumComm != "") && (m.AnnoComm != anno || m.NumComm != numero))
            throw new UserError(Testo(169));
        string sem = r.S("semaforo");
        if (sem != "Y" && sem != "G")
        {
            if (S.BLOCCA_SE_SEMAFORO_ROSSO) throw new UserError(Testo(170));
            if (!Si(r, "171")) return Ok();
        }
        int num = Convert.ToInt32(anno), num2 = Convert.ToInt32(numero);
        if (S.PRELIEVO_MATERIALI == "Y")
        {
            using (DBContext db = DbT(m))
            {
                var gp = new GestorePrelievi(db, S);
                if (tipo == "S") gp.GeneraListaPrelievoCommeSingola(num, num2);
                else gp.GeneraListaPrelievoCommeRaggruppata(num, num2);
                db.CommitTransaction();
            }
        }
        if (tipo == "S") Versamento.GeneraListaVersamentoCommeSingola(num, num2);
        else Versamento.GeneraListaVersamentoCommeRaggruppata(num, num2);
        bool flag = Funzioni.IsCommessaStampaggio(tipo, num, num2) && !Funzioni.IsNImpronteCommePresenti(num, num2, tipo);
        using (DBContext db = DbT(m))
        {
            new StatoRDDal(db).CreaSeInesistente(m.Cod);
            RDMacchineFunz.SettaCommessaSuMacchina(m.Cod, anno, numero, tipo, db.Connection, db.Transaction);
            if (!string.IsNullOrWhiteSpace(m.Famiglia))
            {
                new GestoreFasi(null, db).CopiaFasiFamigliaSuMacchina(new AnnoNumeroTipoComme(num, num2, tipo), m.Famiglia, m.Cod, S.FasiFamigliaSoloUnaMacchina);
            }
            if (flag)
            {
                GestioneStampaggio.ScriviImpronteStampoRaggruppata(new StatoRDExtended
                {
                    Commessa = new AnnoNumeroTipoComme(num, num2, "R"),
                    CodMacchina = m.Cod,
                    CodDipendente = m.CodDip,
                    CodReparto = m.Reparto
                }, db.Connection, db.Transaction);
            }
            db.CommitTransaction();
        }
        var d = Ok();
        d["operazioni_su_fase"] = S.OPERAZIONI_COMMESSA_SU_FASE ? "Y" : "N";
        d["fatto"] = "Y";
        return d;
    }

    // bSospendiCommessa_Click
    static Dictionary<string, string> CommessaSospendi(Req r)
    {
        Mac m = LoadMac(r.S("mac"));
        if (m.AnnoComm == "" || m.NumComm == "") throw new UserError(Testo(165));
        if (S.OPERAZIONI_COMMESSA_SU_FASE && m.NProgr.HasValue)
            throw new UserError("Sospendere prima la fase in corso");
        using (DBContext db = Db(m))
        {
            using (var cmd = new SqlCommand("SELECT COUNT(*) FROM PD_FASI WHERE PD_IDAZIENDA = @IdAzienda AND PD_DTEINI IS NOT NULL AND PD_DTEFIN IS NULL AND PD_ANNOCOM = @AnnoCommessa AND PD_NUMCOM = @NumeroCommessa AND PD_TIPOCOM = @TipoCommessa AND PD_CODMAC = @CodMacchina ", db.Connection))
            {
                cmd.Parameters.AddWithValue("@IdAzienda", Az);
                cmd.Parameters.AddWithValue("@AnnoCommessa", m.NAnno.Value.ToString());
                cmd.Parameters.AddWithValue("@NumeroCommessa", m.NNum.Value);
                cmd.Parameters.AddWithValue("@TipoCommessa", m.TipoComm);
                cmd.Parameters.AddWithValue("@CodMacchina", m.Cod);
                object o = cmd.ExecuteScalar();
                if (o != DBNull.Value && Convert.ToInt32(o) > 0) throw new UserError(Testo(167));
            }
            RDMacchineFunz.SettaCommessaSuMacchina(m.Cod, null, null, null, db.Connection);
        }
        var d = Ok();
        d["fatto"] = "Y";
        return d;
    }

    // ---------------------------------------------------------------- phases

    // IniziaFase_Click
    static Dictionary<string, string> FaseInizia(Req r)
    {
        Mac m = LoadMac(r.S("mac"));
        if (m.AnnoComm == "" || m.CodDip == "" || m.Cod == "" || r.S("progr") == "") throw new UserError(Testo(174));
        int annoComme = m.NAnno.Value, numComme = m.NNum.Value, num3 = r.I("progr");
        FaseInfo fase = LoadFase(m, num3);
        if (FaseFunz.CheckFaseChiusa(annoComme, numComme, m.TipoComm, num3, m.Cod, SQLServer.Connection)) throw new UserError(Testo(175));
        if (FaseFunz.CheckFaseAperta(annoComme, numComme, m.TipoComm, num3, m.Cod, SQLServer.Connection)) throw new UserError(Testo(176));
        if (S.OrdineAvanzFasi == OrdineAvanzFasi.OrdineFase && r.S("semaforo") == "R")
        {
            if (S.BLOCCA_SE_SEMAFORO_ROSSO) throw new UserError(Testo(177));
            if (!Si(r, "178")) return Ok();
        }
        if (fase.IdTemplate.HasValue && !fase.IdSessione.HasValue)
            throw new UserError("Fase con controlli di misura: per ora avviarla dal programma originale");
        var d = Ok();
        d["fatto"] = "Y";
        if (fase.TipoFaseInt == 1 && m.MsgAttrezzaggio.HasValue) d["info"] = Testo(m.MsgAttrezzaggio.Value);
        else if ((fase.TipoFaseInt == 4 || fase.TipoFaseInt == 2) && m.MsgLavorazione.HasValue) d["info"] = Testo(m.MsgLavorazione.Value);
        using (DBContext db = DbT(m))
        {
            SqlConnection connection = db.Connection;
            SqlTransaction transaction = db.Transaction;
            var fk = new FaseKey { Commessa = new AnnoNumeroTipoComme(annoComme, numComme, m.TipoComm), CodMacchina = m.Cod, Progressivo = num3 };
            new GestoreFasi(S.OPERAZIONE_LAVORO_SU_FASE, db).IniziaFase(fk, m.Reparto, m.CodDip, DateTime.Now);
            RDMacchineFunz.ImpostaFaseUtilizzata(m.Cod, num3.ToString(), connection, transaction);
            if (S.PRELIEVO_MATERIALI == "Y")
            {
                LottoPrelevabile.PrelevaLottiDaNonPrelevare(annoComme, numComme, num3, m.CodDip, m.Cod, db);
                new GestorePrelievi(db, S).InserisciPrelieviGiaEffettuati(annoComme, numComme, num3, m.CodDip, m.Cod);
                int num4 = Convert.ToInt32(fase.TipoFase);
                if ((num4 == 2 || num4 == 4) && !UtilizzoLotti.CheckUtilizzoLottiFasePresente(annoComme, numComme, m.TipoComm, num3, m.Cod, connection, transaction))
                    UtilizzoLotti.GeneraUtilizzoLottiFase(annoComme, numComme, m.TipoComm, num3, m.Cod, connection, transaction);
            }
            SwInterfaceMacchine.OperazioniInizioFase(connection, transaction, m.Cod, annoComme, numComme, m.TipoComm, num3);
            string interfaceType, interfaceParams;
            SwInterfaceMacchine.GetInterfaceTypeParams(connection, transaction, m.Cod, out interfaceType, out interfaceParams);
            if (interfaceType == "MAGICMP") new LottiMacchineDal(db).AggiornaLottoImpostato(m.Cod, 1, null);
            db.CommitTransaction();
        }
        return d;
    }

    // bUtilizzaFase_Click
    static Dictionary<string, string> FaseUtilizza(Req r)
    {
        Mac m = LoadMac(r.S("mac"));
        if (r.S("progr") == "") throw new UserError(Testo(179));
        int progr = r.I("progr");
        using (DBContext db = Db(m))
        using (var cmd = new SqlCommand("SELECT COUNT(*) FROM PD_FASI WHERE PD_PROGR = @P AND PD_ANNOCOM = @A AND PD_NUMCOM = @N AND PD_TIPOCOM = @T AND PD_DTEINI IS NOT NULL AND PD_DTEFIN IS NULL AND PD_IDAZIENDA = @AZ", db.Connection))
        {
            cmd.Parameters.AddWithValue("@P", progr);
            cmd.Parameters.AddWithValue("@A", m.AnnoComm);
            cmd.Parameters.AddWithValue("@N", m.NNum.Value);
            cmd.Parameters.AddWithValue("@T", m.TipoComm);
            cmd.Parameters.AddWithValue("@AZ", Az);
            if (Convert.ToInt32(cmd.ExecuteScalar()) == 0) throw new UserError(Testo(191));
            RDMacchineFunz.ImpostaFaseUtilizzata(m.Cod, progr.ToString(), db.Connection);
        }
        var d = Ok();
        d["fatto"] = "Y";
        return d;
    }

    // ChiudiSospendiFase: T = termina, R = sospendi
    static Dictionary<string, string> FaseChiudi(Req r)
    {
        Mac m = LoadMac(r.S("mac"));
        char funz = r.S("funz") == "R" ? 'R' : 'T';
        if (S.VERSAMENTO_SCARTO_CAMBIO_TURNO)
            throw new UserError("Versamento e scarto al cambio turno: per ora usare il programma originale");
        DateTime dtFine = r.Has("dtfine")
            ? DateTime.ParseExact(r.S("dtfine"), "yyyy-MM-dd HH:mm", CultureInfo.InvariantCulture)
            : DateTime.Now;
        dtFine = new DateTime(dtFine.Year, dtFine.Month, dtFine.Day, dtFine.Hour, dtFine.Minute, 0);

        // terminaFase_ControllaValori
        if (r.S("progr") == "") throw new UserError(Testo(179));
        if (m.NumComm == "" || m.Cod == "" || m.ProgrFase == "" || m.CodDip == "") throw new UserError(Testo(174));
        FaseInfo fase = LoadFase(m, r.I("progr"));
        if (!fase.InCorso) throw new UserError(Testo(funz == 'T' ? 180 : 181));
        if (AnomalieFermi.IsFermiAnomalieNonGiustificatiPresenti(m.NAnno.Value, m.NNum.Value, m.Cod, m.NProgr.Value) && !Si(r, "182"))
            return Ok();
        using (DBContext db = DbT(m))
        {
            if (new DatoProduzioneDal(db).IsFermoAnomaliaMacchinaApertaPresente(m.Cod, 'F'))
            {
                if (!Si(r, "183")) return Ok();
                AnomalieFermi.TerminaAnomalieFermiMacchinaAperti(m.Cod, 'F', dtFine);
                db.CommitTransaction();
            }
        }
        using (DBContext db = DbT(m))
        {
            if (new DatoProduzioneDal(db).IsFermoAnomaliaMacchinaApertaPresente(m.Cod, 'A'))
            {
                if (!Si(r, "184")) return Ok();
                AnomalieFermi.TerminaAnomalieFermiMacchinaAperti(m.Cod, 'A', dtFine);
                db.CommitTransaction();
            }
        }
        if (funz == 'T' && (impostazioni.VISUALIZZA_CONFERMA_CHIUSURA_FASE_PEZZI || S.BLOCCA_CHIUSURA_FASI_NON_FINITE))
        {
            if (!ControlloChiusuraFasePezziVersati(r, m, fase)) return Ok();
        }

        // RaccoltaDatiHelper.MostraPopUpChiusiSospendiFase
        decimal numOperatori = fase.NumAddetti;
        decimal numOre = fase.OraInizio.CompareTo(dtFine) >= 0 ? 0m : decimal.Floor((decimal)dtFine.Subtract(fase.OraInizio).TotalHours);
        decimal numMinuti = dtFine.Subtract(fase.OraInizio).Minutes;
        if (impostazioni.MOSTRA_CONFERMA_FASE)
        {
            if (!r.Has("ore"))
            {
                var p = new Dictionary<string, string>();
                p["titolo"] = Testo(funz == 'T' ? 230 : 231) + " " + fase.Descr.Trim() + "?";
                p["ore"] = numOre.ToString(CultureInfo.InvariantCulture);
                p["minuti"] = numMinuti.ToString(CultureInfo.InvariantCulture);
                p["operatori"] = numOperatori.ToString(CultureInfo.InvariantCulture);
                p["dtfine"] = dtFine.ToString("yyyy-MM-dd HH:mm", CultureInfo.InvariantCulture);
                throw new PopupException(p);
            }
            numOre = r.D("ore");
            numMinuti = r.D("minuti");
            numOperatori = r.D("operatori");
        }
        bool commessaTerminata;
        TerminaFase(r, m, fase, funz, numOperatori, numOre, numMinuti, dtFine, out commessaTerminata);
        var d = Ok(impostazioni.MOSTRA_CONFERMA_FASE ? "124" : null);
        d["commessa_terminata"] = commessaTerminata ? "Y" : "N";
        d["fatto"] = "Y";
        return d;
    }

    // fase_ControlloChiusuraFasePezziVersati
    static bool ControlloChiusuraFasePezziVersati(Req r, Mac m, FaseInfo fase)
    {
        if (!Funzioni.IsFaseDiProduzione(fase.TipoFase)) return true;
        bool stampaggio = Funzioni.IsCommessaStampaggio(m.TipoComm, m.NAnno.Value, m.NNum.Value);
        decimal num = Funzioni.getNumPezziDaProdurre(m.NAnno.Value, m.NNum.Value, m.TipoComm);
        decimal num2;
        using (BaseDbContext b = BaseDbContext.GetNewDbContext())
        {
            num2 = !stampaggio
                ? Funzioni.GetNumPezzi(m.AnnoComm, m.NumComm, m.ProgrFase, m.TipoComm, b.Connection, false, null)
                : Funzioni.GetNumColpiStampaggio(m.TipoComm, m.AnnoComm, m.NumComm, m.ProgrFase, b.Connection);
        }
        bool finito = num - num2 <= 0m;
        if (finito && !S.BLOCCA_CHIUSURA_FASI_NON_FINITE) return true;
        if (S.BLOCCA_CHIUSURA_FASI_NON_FINITE)
        {
            bool blocca = false;
            string message = null;
            switch (S.CRITERIO_BLOCCO_CHIUSURA_FASI)
            {
                case "+": blocca = num2 > num; message = "456"; break;
                case "-": blocca = num2 < num; message = "457"; break;
                case "<>": blocca = num2 != num; message = "458"; break;
            }
            if (!blocca) return true;
            if (!Si(r, message)) return false;
            using (DBContext db = Db(m))
            {
                bool ok = new PermessoDal(db, m.IdGruppo, impostazioni.PERMESSO_VALORE_DEFAULT).GetPermesso("MES", "CLOSFASENT").IsAbilitato;
                if (!ok) throw new UserError(Testo(450));
            }
            return true;
        }
        if (impostazioni.VISUALIZZA_CONFERMA_CHIUSURA_FASE_PEZZI) return Si(r, "453");
        return true;
    }

    // checkFasiCommeChiuse
    static bool FasiCommeChiuse(Mac m, SqlTransaction trs)
    {
        using (var cmd = new SqlCommand("SELECT COUNT(*) FROM PD_FASI WHERE PD_IDAZIENDA = @PD_IDAZIENDA AND PD_ANNOCOM = @PD_ANNOCOM AND PD_NUMCOM = @PD_NUMCOM AND PD_TIPOCOM = @PD_TIPOCOM AND PD_CODMAC = @PD_CODMAC AND PD_DTEFIN IS NULL ", trs.Connection, trs))
        {
            cmd.Parameters.AddWithValue("@PD_IDAZIENDA", Az);
            cmd.Parameters.AddWithValue("@PD_ANNOCOM", m.AnnoComm);
            cmd.Parameters.AddWithValue("@PD_NUMCOM", m.NumComm);
            cmd.Parameters.AddWithValue("@PD_TIPOCOM", m.TipoComm);
            cmd.Parameters.AddWithValue("@PD_CODMAC", m.Cod);
            if (m.ProgrFase != "")
            {
                cmd.CommandText += " AND PD_PROGR <> @PD_PROGR ";
                cmd.Parameters.AddWithValue("@PD_PROGR", m.ProgrFase);
            }
            object o = cmd.ExecuteScalar();
            return !(o != null && o != DBNull.Value && Convert.ToInt32(o) > 0);
        }
    }

    // terminaFase_TerminaFase
    static void TerminaFase(Req r, Mac m, FaseInfo fase, char funz, decimal numOperatori, decimal numOre, decimal numMinuti, DateTime dtFine, out bool commessaTerminata)
    {
        commessaTerminata = false;
        var operatori = new List<string> { m.CodDip.TrimEnd() };
        using (DBContext db = DbT(m))
        {
            double totalSeconds = new TimeSpan((int)numOre, (int)numMinuti, 0).TotalSeconds;
            TimeSpan value = new TimeSpan(0, 0, (int)totalSeconds);
            DateTime value2 = dtFine;
            bool flag = true, chiudiCommessa = false;
            var commessa = new AnnoNumeroTipoComme(m.NAnno.Value, m.NNum.Value, m.TipoComm);
            var faseKey = new FaseKey { Commessa = commessa, Progressivo = m.NProgr.Value, CodMacchina = m.Cod };
            var gia = new GestoreInterventiAggiuntivi(db);
            foreach (DatoProduzione item in gia.GetInterventiAggiuntiviApertiFase(faseKey))
            {
                operatori.Add(item.CodOperatore);
                gia.FineInterventoAggiuntivo(item.Progressivo, item.AnnoCommeR.Value, item.NumCommeR.Value, value2);
            }
            value2 = fase.OraInizio.Add(value);
            if (funz == 'T')
            {
                if (S.FasiFamigliaSoloUnaMacchina) new FaseDal(db).ChiudiFaseSuAltreMacchine(faseKey, fase.OraInizio, value2);
                FasiRestanti fr = Funzioni.StatoFasi(m.AnnoComm, m.NumComm, m.TipoComm, m.Cod, fase.Progr.ToString(), db.Connection, db.Transaction);
                if (S.FasiFamigliaSoloUnaMacchina)
                {
                    if (fr == FasiRestanti.UltimaFaseSuFamiglia) fr = FasiRestanti.UltimaFase;
                }
                else if (S.CHIUDI_FASI_ALTRE_MACCHINE_FAMIGLIA && (fr == FasiRestanti.UltimaFaseSuFamiglia || fr == FasiRestanti.FasiSospeseSuFamigliaMaChiuseSuMacchina))
                {
                    List<FaseKey> aperte = FasiAperteMacchineDiFamiglia(faseKey, m, db);
                    if (aperte.Count > 0 && Si(r, "466"))
                    {
                        var fd = new FaseDal(db);
                        foreach (FaseKey k in aperte) fd.AggiornaDtInizioDtFineFlgUltima(k, fase.OraInizio, value2);
                        fr = FasiRestanti.UltimaFase;
                    }
                }
                switch (fr)
                {
                    case FasiRestanti.UltimaFase:
                        if (Si(r, "188"))
                        {
                            chiudiCommessa = true;
                            if (!FasiCommeChiuse(m, db.Transaction)) throw new UserError(Testo(164));
                        }
                        else flag = false;
                        break;
                    case FasiRestanti.UltimaFaseSuFamiglia:
                        string a = Chiedi(r, "189");
                        if (a == "S")
                        {
                            chiudiCommessa = true;
                            if (!FasiCommeChiuse(m, db.Transaction)) throw new UserError(Testo(164));
                        }
                        else if (a == "A") flag = false;
                        break;
                }
            }
            if (!flag) return;
            bool lastLav = FaseFunz.IsLastLavFase(m.NAnno.Value, m.NNum.Value, m.TipoComm, m.NProgr.Value, db.Connection, db.Transaction);
            if (funz == 'T' && lastLav && S.PRELIEVO_MATERIALI == "Y" && m.TipoComm == "R")
            {
                if (!string.IsNullOrWhiteSpace(S.CAUSALE_SCARTO_MATERIALE_FINE_COMME))
                {
                    string art = new GestoreCommesse(db).GetArticoloCommessa(commessa);
                    List<Prelievo> prel = Prelievo.GetPrelieviComme(m.TipoComm, m.NAnno.Value, m.NNum.Value, db.Connection, db.Transaction);
                    decimal tot = 0m;
                    foreach (Prelievo p in prel)
                        tot += p.Qta - Reso.GetQtaResa(m.TipoComm, m.NAnno.Value, m.NNum.Value, p.CodArt, p.IdLotto, null, db.Connection, db.Transaction);
                    decimal versata = Versamento.GetQtaVersataArticolo(m.TipoComm, m.NAnno.Value, m.NNum.Value, -1, art, db.Connection, db.Transaction);
                    decimal diff = tot - versata;
                    if (diff > 0m)
                    {
                        new Scarto
                        {
                            CodArt = art,
                            IdLotto = null,
                            Qta = diff,
                            AnnoComme = m.NAnno.Value,
                            NumComme = m.NNum.Value,
                            ProgrFase = m.NProgr.Value,
                            CodMacchina = m.Cod,
                            CodOperazione = S.CAUSALE_SCARTO_MATERIALE_FINE_COMME,
                            AnnoCommeRag = m.NAnno.Value,
                            NumCommeRag = m.NNum.Value,
                            CodDip = m.CodDip
                        }.ScriviScarto(db);
                    }
                }
                else
                {
                    Reso.RendiMaterialiNonConsumati(m.TipoComm, m.NAnno.Value, m.NNum.Value, m.NProgr.Value, m.Cod, m.CodDip, db);
                }
            }
            if (funz == 'T') new GestoreFasi(S.OPERAZIONE_LAVORO_SU_FASE, db).TerminaFase(faseKey, value2, numOperatori, totalSeconds);
            else new GestoreFasi(S.OPERAZIONE_LAVORO_SU_FASE, db).SospendiFase(faseKey, value2, numOperatori, totalSeconds);
            if (chiudiCommessa)
            {
                ChiudiCommessa(m, db.Transaction);
                commessaTerminata = true;
            }
            if (TornaScheda)
            {
                List<TurnoAperto> turni = TurniAperti();
                foreach (string codOp in operatori)
                {
                    TurnoAperto t = turni.FirstOrDefault(x => x.Cod == codOp);
                    if (t == null) continue;
                    TimeSpan ts = value2.Subtract(t.Inizio);
                    new DatoProduzioneDal(db).Insert(new DatoProduzione
                    {
                        DTInizio = t.Inizio,
                        CodOperazione = S.OPERAZIONE_LAVORO_TURNI,
                        DTFine = value2,
                        CodOperatore = codOp,
                        TotSecondi = (int)ts.TotalSeconds
                    });
                    new TurnoDal(db).FineTurno(codOp, value2);
                    new StatoRDDal(db).TogliOperatoreDaMacchine(codOp);
                }
            }
            RDMacchineFunz.ImpostaFaseUtilizzata(m.Cod, "NULL", db.Connection, db.Transaction);
            if (chiudiCommessa)
                Funzioni.TogliCommessaDaTutteLeMacchine(m.AnnoComm, m.NumComm, m.TipoComm, db.Transaction);
            else if (!new FaseDal(db).CheckFasiNonChiuseCommeMac(faseKey.Commessa, m.Cod, m.Famiglia))
                Funzioni.TogliCommessaDaMacchina(m.Cod, db.Connection, db.Transaction);
            db.CommitTransaction();
        }
    }

    // ChiudiCommessa
    static void ChiudiCommessa(Mac m, SqlTransaction trs)
    {
        try
        {
            Commessa.ChiudiCommessa(m.NAnno.Value, m.NNum.Value, m.TipoComm, m.CodDip, m.Cod, m.Reparto, trs.Connection, trs);
        }
        catch (Exception ex)
        {
            Configurazioni.ExMessage = ": " + ex.Message;
            throw new UserError(Testo(166) + " " + Configurazioni.ExMessage);
        }
    }

    // getListaFasiAperteMacchineDiFamiglia
    static List<FaseKey> FasiAperteMacchineDiFamiglia(FaseKey origine, Mac m, DBContext db)
    {
        var list = new List<FaseKey>();
        using (SqlCommand cmd = db.GetNewCommand())
        {
            cmd.CommandText = @"SELECT PD_PROGR, PD_CODMAC FROM PD_FASI INNER JOIN CELPRO ON (CE_IDAZIENDA = PD_IDAZIENDA AND CELPRO.CODICE = PD_CODMAC)
WHERE PD_IDAZIENDA = @PD_IDAZIENDA AND PD_ANNOCOM = @PD_ANNOCOM AND PD_NUMCOM = @PD_NUMCOM AND PD_TIPOCOM = @PD_TIPOCOM AND CE_FLGFAMIGLIA = 'Y'";
            cmd.Parameters.AddWithValue("@PD_IDAZIENDA", db.IdAzienda);
            cmd.Parameters.AddWithValue("@PD_ANNOCOM", origine.Commessa.AnnoComme);
            cmd.Parameters.AddWithValue("@PD_NUMCOM", origine.Commessa.NumComme);
            cmd.Parameters.AddWithValue("@PD_TIPOCOM", origine.Commessa.TipoComme);
            var famiglie = new List<int>();
            using (var dr = cmd.ExecuteReader())
            {
                while (dr.Read()) famiglie.Add(Convert.ToInt32(dr["PD_PROGR"]));
            }
            foreach (int progr in famiglie)
            {
                cmd.CommandText = @"SELECT PD_CODMAC FROM PD_FASI INNER JOIN CELPRO ON (CE_IDAZIENDA = PD_IDAZIENDA AND CELPRO.CODICE = PD_CODMAC)
WHERE PD_IDAZIENDA = @PD_IDAZIENDA AND PD_ANNOCOM = @PD_ANNOCOM AND PD_NUMCOM = @PD_NUMCOM AND PD_TIPOCOM = @PD_TIPOCOM AND PD_PROGR = @PD_PROGR
 AND NOT (PD_CODMAC = @PD_CODMAC AND PD_PROGR = @PD_PROGR_ORIGINE) AND PD_DTEFIN IS NULL AND PD_DTEINI IS NULL AND CE_FLGFAMIGLIA <> 'Y'";
                cmd.Parameters.Clear();
                cmd.Parameters.AddWithValue("@PD_IDAZIENDA", db.IdAzienda);
                cmd.Parameters.AddWithValue("@PD_ANNOCOM", origine.Commessa.AnnoComme);
                cmd.Parameters.AddWithValue("@PD_NUMCOM", origine.Commessa.NumComme);
                cmd.Parameters.AddWithValue("@PD_TIPOCOM", origine.Commessa.TipoComme);
                cmd.Parameters.AddWithValue("@PD_PROGR", progr);
                cmd.Parameters.AddWithValue("@PD_CODMAC", origine.CodMacchina);
                cmd.Parameters.AddWithValue("@PD_PROGR_ORIGINE", origine.Progressivo);
                using (var dr = cmd.ExecuteReader())
                {
                    while (dr.Read())
                        list.Add(new FaseKey { Commessa = origine.Commessa, CodMacchina = dr["PD_CODMAC"].ToString().Trim(), Progressivo = progr });
                }
            }
        }
        return list;
    }
}
