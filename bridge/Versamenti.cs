// Recording produced quantities: ESP_FormMain.bNewPezzi_Click -> ESP_FormPezziRag
// (load + b_Conferma) -> ESP_FormConfConsumiMat (checks on OK) -> write.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Esprit.Entities;
using Esprit.Logic;
using Esprit.Sys.Dal;
using Microsoft.Data.SqlClient;
using RD.Dal.MSSql;
using RD.Entities;
using RD.Logic;

class VersRiga
{
    public int Anno, Num;
    public string Tipo, CodArt, Descr, UM;
    public decimal QtaDaFare, QtaFasePrec;
    public int NDec;
    public Articolo Art;
}

class VersStato
{
    public Mac M;
    public bool AvanzaMagazzino;
    public string DescrFase;
    public string Label;
    public decimal? Coeff;
    public int ProgFasePrec;
    public bool CheckVersFasePrec;
    public bool SuSingole;
    public string CodiceOpe;
    public bool UpdFlgUltimaFase;
    public Dictionary<annoNumeroTipoComme, DatiCommessa> Dk;
    public List<VersRiga> Righe = new List<VersRiga>();
    public List<Articolo> Articoli = new List<Articolo>();
    public List<DBase> MaterialiAgg = new List<DBase>();
}

static partial class Ponte
{
    const char RS = '\u001e';
    const char US = '\u001f';

    static string N(decimal d) { return d.ToString(CultureInfo.InvariantCulture); }

    // bNewPezzi_Click + RegistraPezzi pre-checks, then the form's constructor.
    static VersStato VersPrepara(Req r)
    {
        Mac m = LoadMac(r.S("mac"));
        if (m.NumComm == "" || m.Cod == "" || m.ProgrFase == "" || m.CodDip == "") throw new UserError(Testo(174));
        int tipoFase = Funzioni.GetTipoFase(m.CodFase);
        if (tipoFase == 3 || tipoFase == 1) throw new UserError(Testo(206));
        bool avanza = Funzioni.IsLastLavFase(m.AnnoComm, m.NumComm, m.TipoComm, m.ProgrFase);
        if (m.TipoComm == "S") throw new UserError(Testo(207));
        if (r.S("registra_pesate") == "Y") throw new UserError("Registrazione pesate: per ora usare il programma originale");
        if (tipoFase == 2) throw new UserError("Fasi di stampaggio: per ora usare il programma originale");
        if (S.CONTAPEZZI_LOTTI_MACCHINA && m.SwInterfaceType == "MAGICMP") throw new UserError("Versamento da contapezzi lotti macchina: per ora usare il programma originale");

        var v = new VersStato { M = m, AvanzaMagazzino = avanza };
        int anno = m.NAnno.Value, num = m.NNum.Value, progr = m.NProgr.Value;
        using (SqlConnection con = SQLServer.getNewConn())
        {
            using (var cmd = new SqlCommand("SELECT PD_CODFAS, PD_DESFAS, PD_LABEL, PD_COEFFICIENTE FROM PD_FASI WHERE PD_ANNOCOM = @A AND PD_NUMCOM = @N AND PD_TIPOCOM = @T AND PD_PROGR = @P AND PD_IDAZIENDA = @AZ", con))
            {
                cmd.Parameters.AddWithValue("@A", m.AnnoComm);
                cmd.Parameters.AddWithValue("@N", num);
                cmd.Parameters.AddWithValue("@T", m.TipoComm);
                cmd.Parameters.AddWithValue("@P", progr);
                cmd.Parameters.AddWithValue("@AZ", Az);
                using (var dr = cmd.ExecuteReader())
                {
                    if (!dr.Read()) throw new UserError(Testo(142));
                    v.DescrFase = Str(dr["PD_DESFAS"]).Trim();
                    v.Label = dr["PD_LABEL"] == DBNull.Value ? null : dr["PD_LABEL"].ToString().Trim();
                    v.Coeff = dr["PD_COEFFICIENTE"] == DBNull.Value ? (decimal?)null : Convert.ToDecimal(dr["PD_COEFFICIENTE"]);
                    if (string.IsNullOrEmpty(v.Label) || !v.Coeff.HasValue) { v.Label = null; v.Coeff = null; }
                }
            }
            foreach (string ope in new[] { S.CAUSALE_VERS_FASE_INTERM, S.CAUSALE_VERS_FASE_ULTIMA })
            {
                using (var cmd = new SqlCommand("SELECT PD_DESOPE FROM PD_OPERAZIONI WHERE PD_CODOPE = @C AND PD_IDAZIENDA = @AZ", con))
                {
                    cmd.Parameters.AddWithValue("@C", ope);
                    cmd.Parameters.AddWithValue("@AZ", Az);
                    using (var dr = cmd.ExecuteReader()) if (!dr.Read()) throw new UserError(Testo(143));
                }
            }
            // RefreshFasi: AppState.iLastLavFase
            string lastLav = "";
            using (var cmd = new SqlCommand("SELECT MAX(PD_PROGR) FROM PD_FASI, PD_TIPIFASI WHERE PD_ANNOCOM = @A AND PD_NUMCOM = @N AND PD_TIPOCOM = @T AND PD_FASI.PD_CODFAS = PD_TIPIFASI.PD_CODFAS AND (PD_TYP = 2 OR PD_TYP = 4) AND PD_FASI.PD_IDAZIENDA = PD_TIPIFASI.PD_IDAZIENDA AND PD_FASI.PD_IDAZIENDA = @AZ", con))
            {
                cmd.Parameters.AddWithValue("@A", m.AnnoComm);
                cmd.Parameters.AddWithValue("@N", num);
                cmd.Parameters.AddWithValue("@T", m.TipoComm);
                cmd.Parameters.AddWithValue("@AZ", Az);
                object o = cmd.ExecuteScalar();
                try { lastLav = Convert.ToDecimal(o).ToString(); } catch (Exception) { lastLav = ""; }
                lastLav = Intero(lastLav);
            }
            // RaccoltaDatiHelper.GetCodOperazioneVersamenti
            if (S.OrdineAvanzFasi == OrdineAvanzFasi.OrdineFase)
            {
                if (m.ProgrFase == lastLav) { v.CodiceOpe = S.CAUSALE_VERS_FASE_ULTIMA; v.UpdFlgUltimaFase = true; }
                else v.CodiceOpe = S.CAUSALE_VERS_FASE_INTERM;
            }
            else if (avanza) { v.CodiceOpe = S.CAUSALE_VERS_FASE_ULTIMA; v.UpdFlgUltimaFase = true; }
            else v.CodiceOpe = S.CAUSALE_VERS_FASE_INTERM;
        }

        using (DBContext db = Db(m))
        {
            v.ProgFasePrec = FaseFunz.GetProgrFasePrec(anno, num, m.TipoComm, progr, db.Connection, db.Transaction, true);
            v.CheckVersFasePrec = new GestoreCommesse(db).CheckCommessaControllaVersFasePrec(m.Commessa, S.CONTROLLA_PEZZI_FASE_PREC) && v.ProgFasePrec > 0;
            if ((v.CheckVersFasePrec || S.MES_VERSAMENTO_SHOW_QTA_FASE_PREC) && S.VERSA_SU_SINGOLE)
                throw new UserError("Controllo pezzi fase precedente non gestito per versamenti su singole!");
        }

        // CaricaDGVersamenti
        v.SuSingole = S.VERSA_SU_SINGOLE;
        bool soloBuoni = m.VersaSoloPezziBuoni;
        if (v.SuSingole && S.NASCONDI_COMMESSE_COMPLETE_VERSAMENTO) soloBuoni = true;
        Dictionary<annoNumeroTipoComme, DatiCommessa> dk;
        Dictionary<string, decimal> articoli = Versamento.GetArticoliDaProdurre(anno, num, m.TipoComm, progr, r.S("ins_parziale") == "S", false, avanza, soloBuoni, out dk);
        v.Dk = dk;
        if (dk.Count == 1 && v.SuSingole) v.SuSingole = false;
        if (m.SwInterfaceType == "MAGICMP")
        {
            using (DBContext db = Db(m))
            {
                string parametri, x;
                SwInterfaceMacchine.GetParametriFase(db.Connection, db.Transaction, anno, num, m.TipoComm, progr, m.Cod, out parametri, out x);
                decimal res;
                if (!string.IsNullOrEmpty(parametri) && decimal.TryParse(parametri, out res))
                {
                    articoli[articoli.Keys.First()] += res;
                    dk[dk.Keys.First()].QtaComme += res;
                }
            }
        }
        using (DBContext db = Db(m))
        {
            var gc = new GestoreCommesse(db);
            var ab = new ArticoloBiz(db);
            var fasePrec = new List<CodiceValore>();
            if (v.CheckVersFasePrec || S.MES_VERSAMENTO_SHOW_QTA_FASE_PREC)
            {
                fasePrec = new GestoreVersamenti(db).GetQtaVersabiliRispettoFasePrec(
                    new FaseKey { Commessa = m.Commessa, Progressivo = progr, CodMacchina = m.Cod },
                    new FaseKey { Commessa = m.Commessa, Progressivo = v.ProgFasePrec, CodMacchina = m.Cod }, S);
            }
            if (v.SuSingole)
            {
                foreach (annoNumeroTipoComme k in dk.Keys.OrderBy(c => c.annoComme).ThenBy(c => c.numComme))
                {
                    DatiCommessa dc = dk[k];
                    if (S.NASCONDI_COMMESSE_COMPLETE_VERSAMENTO && dc.QtaComme == 0m)
                    {
                        Articolo a = ab.GetSingleByCod(dc.CodArtComme);
                        if (!v.Articoli.Contains(a)) v.Articoli.Add(a);
                        continue;
                    }
                    v.Righe.Add(Riga(v, k.annoComme, k.numComme, k.tipoComme, dc.QtaComme, dc.CodArtComme, ab, gc, fasePrec));
                }
            }
            else
            {
                foreach (string codArt in articoli.Keys.OrderBy(x => x))
                    v.Righe.Add(Riga(v, anno, num, m.TipoComm, articoli[codArt], codArt, ab, gc, fasePrec));
            }
        }

        // CaricaDGMaterialeCons: this first version covers phases without material
        // consumption, lots to pick or semi-finished lots (none are used at this site).
        if (S.PRELIEVO_MATERIALI == "Y")
        {
            bool consumi = false;
            using (DBContext db = Db(m))
            {
                foreach (AnnoNumeroTipoComme item in new CommessaDal(db).GetCommesseRaggruppamento(anno, num))
                {
                    var key = new annoNumeroTipoComme { annoComme = item.AnnoComme, numComme = item.NumComme, tipoComme = item.TipoComme };
                    if (!dk.ContainsKey(key)) continue;
                    dk[key].LMaterialiNec = Consumo.GetMaterialeDaConsumareFase(key.tipoComme, key.annoComme, key.numComme, progr);
                    dk[key].LMaterialiFacoltativi = Consumo.GetMaterialeDaConsumareFase(key.tipoComme, key.annoComme, key.numComme, 0);
                    if (S.CODFASI_CONSUMO_MATERIALE.Count > 0)
                    {
                        if (S.CODFASI_CONSUMO_MATERIALE.Contains(m.CodFase)) dk[key].LMaterialiNec.AddRange(dk[key].LMaterialiFacoltativi);
                        dk[key].LMaterialiFacoltativi.Clear();
                    }
                    if (dk[key].LMaterialiNec.Count > 0 || dk[key].LMaterialiFacoltativi.Count > 0) consumi = true;
                }
                v.MaterialiAgg = Consumo.GetMaterialeDaConsumareAgg(anno, num);
                if (v.MaterialiAgg.Count > 0) consumi = true;
                if (v.ProgFasePrec != -1 && Prelievo.GetLottiSemilavoratiFase("R", anno, num, v.ProgFasePrec, db.Connection).Count > 0) consumi = true;
            }
            if (consumi) throw new UserError("Questa fase consuma materiali o semilavorati: per ora registrare le quantità dal programma originale");
        }
        foreach (Articolo a in v.Articoli)
        {
            if (a != null && a.FlgTracciato != "N")
                throw new UserError("Articolo " + a.Codice.TrimEnd() + " gestito a lotti/matricole: per ora usare il programma originale");
        }
        return v;
    }

    static VersRiga Riga(VersStato v, int anno, int num, string tipo, decimal qta, string codArt, ArticoloBiz ab, GestoreCommesse gc, List<CodiceValore> fasePrec)
    {
        Articolo a = ab.GetSingleByCod(codArt);
        if (!v.Articoli.Contains(a)) v.Articoli.Add(a);
        var riga = new VersRiga { Anno = anno, Num = num, Tipo = tipo, CodArt = codArt, Art = a, QtaDaFare = qta };
        riga.NDec = a.DecimaliQta != int.MaxValue ? a.DecimaliQta : S.NDecQta;
        riga.Descr = (gc.GetDescrArticoloCommessa(new AnnoNumeroTipoComme(anno, num, tipo), a.FlgRigaLibera, a.Descrizione) ?? "").Trim();
        riga.UM = a.UnMis == null ? "" : a.UnMis.TrimEnd();
        CodiceValore cv = fasePrec.FirstOrDefault(x => x.Codice == codArt.TrimEnd());
        riga.QtaFasePrec = cv != null ? cv.Valore : 0m;
        return riga;
    }

    static Dictionary<string, string> VersCarica(Req r)
    {
        VersStato v = VersPrepara(r);
        var d = Ok();
        d["fase"] = v.DescrFase;
        d["ultima"] = v.AvanzaMagazzino ? "Y" : "N";
        d["operazione"] = v.CodiceOpe;
        d["su_singole"] = v.SuSingole ? "Y" : "N";
        d["label"] = v.Label ?? "";
        d["coeff"] = v.Coeff.HasValue ? N(v.Coeff.Value) : "";
        d["fase_prec"] = (v.CheckVersFasePrec || S.MES_VERSAMENTO_SHOW_QTA_FASE_PREC) ? "Y" : "N";
        d["versa_rimanenti"] = S.VERSA_PEZZI_RIMANENTI ? "Y" : "N";
        d["solo_buoni"] = v.M.VersaSoloPezziBuoni ? Testo(507) : "";
        var rimanenti = new Dictionary<VersRiga, decimal>();
        if (S.VERSA_PEZZI_RIMANENTI)
        {
            using (SqlConnection con = SQLServer.getNewConn())
                foreach (VersRiga x in v.Righe) rimanenti[x] = Rimanenti(x, v.M.ProgrFase, con);
        }
        d["righe"] = string.Join(RS.ToString(), v.Righe.Select(x => string.Join(US.ToString(), new[] {
            x.Anno.ToString(), x.Num.ToString(), x.Tipo, x.CodArt.TrimEnd(), x.Descr, N(x.QtaDaFare), x.UM, x.NDec.ToString(), N(x.QtaFasePrec),
            N(rimanenti.ContainsKey(x) ? rimanenti[x] : x.QtaDaFare)
        })));
        return d;
    }

    // ESP_FormPezziRag.bVersaTutto_Click with VERSA_PEZZI_RIMANENTI: quantity still missing per row.
    static decimal Rimanenti(VersRiga x, string progrFase, SqlConnection con)
    {
        decimal fatti = Funzioni.GetNumPezzi(x.Anno.ToString(), x.Num.ToString(), progrFase, "S", con, false, null);
        return x.QtaDaFare - fatti > 0m ? x.QtaDaFare - fatti : 0m;
    }

    // b_Conferma_Click (ESP_FormPezziRag) + b_Conferma_Click (ESP_FormConfConsumiMat) + write.
    static Dictionary<string, string> VersConferma(Req r)
    {
        VersStato v = VersPrepara(r);
        Mac m = v.M;
        // righe = "anno|num|codart|qta" separated by RS
        var qta = new Dictionary<string, decimal>();
        foreach (string row in r.S("righe").Split(RS))
        {
            string[] c = row.Split(US);
            if (c.Length < 4) continue;
            qta[c[0] + "-" + c[1] + "-" + c[2].Trim()] = decimal.Parse(c[3], CultureInfo.InvariantCulture);
        }
        List<DatiCommessa> datiCommesse = v.Dk.Values.ToList();
        VersamentoProcessor vp = new VersamentoProcessor().Initialize().SetCodOpeVersamento(v.CodiceOpe).SetProgrFaseVersamentoPrecedente(v.ProgFasePrec)
            .SetCommessaFaseMacchinaOperatore(m.NAnno.Value, m.NNum.Value, m.NProgr.Value, m.Cod, m.CodDip)
            .SetStampaggio(false)
            .SetDatiCommesse(datiCommesse)
            .SetDtInserimento(DateTime.Now);

        // GetArticoliDaVersare
        bool almenoUno = false;
        foreach (VersRiga x in v.Righe)
        {
            decimal num;
            if (!qta.TryGetValue(x.Anno + "-" + x.Num + "-" + x.CodArt.Trim(), out num)) continue;
            num = decimal.Round(num, x.NDec);
            decimal num2 = decimal.Round(x.QtaDaFare, x.NDec);
            if (num > 0m && S.BLOCCA_VERSAMENTO_MAGGIORE_COMMESSA != "N" && num > num2)
            {
                if (S.BLOCCA_VERSAMENTO_MAGGIORE_COMMESSA == "Y") throw new UserError(Testo(460));
                if (S.BLOCCA_VERSAMENTO_MAGGIORE_COMMESSA == "F")
                {
                    if (!Si(r, "459")) return Ok();
                    using (DBContext db = Db(m))
                    {
                        if (!new PermessoDal(db, m.IdGruppo, impostazioni.PERMESSO_VALORE_DEFAULT).GetPermesso("MES", "VERSQTAMAG").IsAbilitato)
                            throw new UserError(Testo(450));
                    }
                }
            }
            if (v.CheckVersFasePrec && num > x.QtaFasePrec) throw new UserError(Testo(471));
            if (num > 0m)
            {
                almenoUno = true;
                bool matricole = x.Art.FlgTracciato == "M";
                if (matricole && decimal.Truncate(num) != num) throw new UserError(Testo(150) + " " + x.CodArt);
                if (v.SuSingole) vp.AddArticoloDaVersare(new AnnoNumeroTipoComme(x.Anno, x.Num, "S"), x.CodArt, num, matricole, 0m);
                else vp.AddArticoloDaVersare(x.CodArt, num, matricole, 0m);
            }
        }
        if (!almenoUno) throw new UserError(Testo(461));

        List<VersamentoProposto> proposti;
        using (DBContext db = Db(m))
        {
            proposti = vp.SetDbContext(db).GeneraVersamentiProposti().GetVersamentiProposti();
            if (S.PRELIEVO_MATERIALI == "Y")
            {
                // No material rows on screen (checked in VersPrepara): every needed
                // material is missing, as the original reports with message 227.
                foreach (VersamentoProposto vers in proposti)
                {
                    DatiCommessa dc = datiCommesse.First(d => d.CommessaKey.AnnoComme == vers.AnnoComme && d.CommessaKey.NumComme == vers.NumComme);
                    if (dc.LMaterialiNec != null && dc.LMaterialiNec.Count > 0)
                        throw new UserError(Testo(227) + " " + dc.LMaterialiNec[0].CodArt2 + " / " + vers.CodArt);
                    Articolo a2 = v.Articoli.First(a => a.Codice.TrimEnd() == vers.CodArt);
                    if (vers.IdLotto.HasValue || dc.LIdLottiDaAssegnare.Count > 0 || a2.FlgTracciato == "Y")
                        throw new UserError("Articolo " + vers.CodArt + " gestito a lotti: per ora usare il programma originale");
                }
            }
            // ESP_FormConfConsumiMat.b_Conferma_Click checks
            foreach (VersamentoProposto item in proposti)
            {
                if (!item.IdLotto.HasValue)
                {
                    var c = new AnnoNumeroTipoComme { AnnoComme = item.AnnoComme, NumComme = item.NumComme, TipoComme = "S" };
                    if (new LottoDaAssegnareDal(db).List(c).Count > 0) throw new UserError(Testo(139) + " " + item.CodArt);
                }
            }
        }

        using (DBContext db = DbT(m))
        {
            if (v.UpdFlgUltimaFase)
            {
                new FaseDal(db).AggiornaFlgFaseUltima(new FaseKey
                {
                    Commessa = new AnnoNumeroTipoComme { AnnoComme = m.NAnno.Value, NumComme = m.NNum.Value, TipoComme = m.TipoComm },
                    Progressivo = m.NProgr.Value
                }, true);
            }
            int durata = new FaseDal(db).GetSingle(new FaseKey
            {
                Commessa = new AnnoNumeroTipoComme { AnnoComme = m.NAnno.Value, NumComme = m.NNum.Value, TipoComme = m.TipoComm },
                Progressivo = m.NProgr.Value,
                CodMacchina = m.Cod
            }).Durata;
            vp.SetDbContext(db).SetVersamentiProposti(proposti).CalcolaTempoVersamenti(S.OPERAZIONE_LAVORO_SU_FASE, m.Famiglia)
                .SetTempoFaseTeoricoVersamenti(durata)
                .ScriviVersamenti()
                .ScriviVersamentiAggiuntivi(null)
                .ScriviConsumi();
            new CommessaDal(db).AggiornaDtModT(new AnnoNumeroTipoComme { AnnoComme = m.NAnno.Value, NumComme = m.NNum.Value, TipoComme = m.TipoComm });
            db.CommitTransaction();
        }
        var res = Ok("124");
        res["righe_scritte"] = proposti.Count.ToString();
        if (v.AvanzaMagazzino && S.LIndirizziMailVersamentoFiniti.Count > 0)
        {
            var sb = new System.Text.StringBuilder();
            sb.Append("Effettuati i seguenti versamenti di finiti: <br />");
            sb.Append("<table border='2' cellpadding='7'><tr><th>Anno commessa</th><th>Numero commessa</th><th>Articolo</th><th>Matricola</th><th>Quantità</th></tr>");
            foreach (VersamentoProposto p in proposti)
            {
                string mat = "";
                if (p.IdLotto.HasValue) { Lotto l = Lotto.GetLotto(p.IdLotto.Value); if (l != null) mat = l.Matricola; }
                sb.Append("<tr><td>" + p.AnnoComme + "</td><td>" + p.NumComme + "</td><td>" + p.CodArt + "</td><td>" + mat + "</td><td>" + p.Qta + "</td></tr>");
            }
            sb.Append("</table><br />");
            string error;
            var mail = new MailSender(S.MailSmtp, S.MailSmtpPort, S.MailUseSSL, S.MailSender, S.MailPassword);
            if (!mail.Send("Notifica versamento finiti", sb.ToString(), true, S.LIndirizziMailVersamentoFiniti, out error))
                res["avviso"] = Testo(252) + " " + error;
        }
        return res;
    }
}
