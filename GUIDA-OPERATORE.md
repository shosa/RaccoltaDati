# Raccolta Dati — Guida per l'operatore

Questa app sostituisce "Raccolta dati" di Esprit sul monitor touch. Lavora sullo **stesso
database** e registra con le **stesse regole** del programma originale: quello che fai qui lo
vedi anche nel programma originale e viceversa.

---

## 1. Avvio

1. Apri **RaccoltaDati** con **un solo clic** e attendi. Se la finestra non compare subito,
   l'app riprova da sola (fino a circa un minuto): **non cliccare di nuovo**.
2. Compare la schermata di caricamento con i passi (server, database, macchine…). Se un passo
   diventa rosso, leggi il messaggio e avvisa chi segue il programma.
3. Si apre la schermata **Accesso operatore**.

> Nella schermata di accesso, in basso a destra, c'è **IMPOSTAZIONI**: serve solo a chi
> configura la postazione (vedi il capitolo 8).

## 2. Accesso e turno

- Premi **ACCEDI** e scrivi il tuo codice operatore sulla tastiera (lettere e numeri), poi **OK**.
  Se richiesta, inserisci la password.
- Se il tuo turno non è aperto, l'app lo apre. Se è già aperto (ad esempio dopo un riavvio),
  l'app ti fa semplicemente rientrare.
- Finché nessuno ha fatto l'accesso, l'app non mostra dati né menu.

## 3. Prendere una macchina

1. Dopo l'accesso si apre **MACCHINE**. Tocca la tua macchina (la scheda diventa blu).
2. Premi **PRENDI MACCHINA**: da ora la macchina è sotto il tuo controllo.

La lista si scorre **trascinando con il dito**, oppure con le frecce **▲ ▼**. Il pallino verde
indica una macchina con una commessa in lavorazione, quello rosso una macchina libera.

## 4. Il lavoro normale

Le schede in alto sono: **OPERATORI**, **MACCHINE**, **COMMESSE** (con le sotto-schede COMMESSE e
FASI), **ARTICOLI**, il pulsante verde-acqua con la **lente** (INFO COMMESSA) e il pulsante
arancione **BARCODE**.

**Commesse.** Tocca una commessa e premi **INIZIA** per avviarla sulla macchina. **SOSPENDI**
la toglie dalla macchina, **TERMINA** chiude la fase in uso, **DETTAGLI** mostra gli articoli.
Il filtro (imbuto) cerca una commessa per Anno-Numero. La lista si scorre con il dito o con ▲ ▼.

**Fasi.** Tocca una fase e usa **INIZIA FASE**, **SOSPENDI FASE**, **TERMINA FASE** o
**UTILIZZA FASE**. Alla sospensione o alla chiusura l'app propone ore, minuti e numero di
operatori: correggi se serve e **CONFERMA**.

**Versare i pezzi.** Da **FASI** premi **VERSA PEZZI**: compare l'elenco delle commesse della fase
con quanto resta da fare. Tocca la casella della quantità e scrivi i pezzi, oppure **VERSA TUTTO**,
poi **CONFERMA**. **PEZZI REGISTRATI** mostra cosa è già stato versato.

## 5. BARCODE — versamento automatico

Il lettore di barcode è **spento** finché non premi **BARCODE**.

1. Premi **BARCODE**: si apre la finestra "Lettore in ascolto".
2. Spara il barcode della commessa (oppure **INSERISCI A MANO**).
3. L'app cerca la commessa **solo sulla macchina che stai controllando**. Se ne trova più di una,
   scegli dall'elenco.
4. L'app apre da sola commessa e fase, ti mostra la quantità **già proposta** (quella ancora da
   fare): correggi con − / + se serve e premi **CONFERMA**.
5. Registrato il versamento, la macchina torna esattamente com'era prima.

Se il codice non va bene (commessa non su questa macchina, già versata completamente…) la
finestra lo dice e resta in ascolto per un altro codice. **ANNULLA** chiude senza registrare.

Nella schermata **VERSA PEZZI** c'è lo stesso pulsante **BARCODE**: lì serve solo a trovare la
riga della commessa e proporre la quantità.

## 6. PIÙ COMMESSE — tante commesse in un colpo solo

1. Premi **BARCODE**, poi il pulsante arancione **PIÙ COMMESSE**.
2. Spara le commesse una dopo l'altra. Ognuna viene **solo controllata** (nulla viene registrato)
   e aggiunta all'elenco con la quantità massima ancora da versare. Puoi ridurla con − / +,
   toccarla per scriverla, o togliere la riga con ✕.
3. Le commesse non valide vengono rifiutate subito con il motivo (non sulla macchina, già
   versate, già in elenco, macchina senza operatore…).
4. Premi **AVVIA VERSAMENTO**. L'app raggruppa le commesse per commessa **R** e per ognuna apre
   la macchina **una sola volta**, versa tutte le sue commesse insieme e rimette la macchina com'era.
5. Alla fine compare il riepilogo: quante sono state registrate e il motivo di quelle non registrate.

> Commesse di commesse R diverse si possono mescolare **solo se la macchina parte libera**.
> Se la macchina sta già lavorando una commessa R, si possono aggiungere solo le commesse di
> quella R (le altre vengono rifiutate allo sparo, come fa il programma originale).

## 7. INFO COMMESSA — su quale macchina si trova?

Premi il pulsante verde-acqua con la **lente** e spara (o scrivi) un codice: l'app mostra le
commesse con quel numero che hanno fasi sulle **macchine di questa postazione**, con la macchina
per prima, la fase, lo stato (da fare, in corso, terminata) e i pezzi già versati.
**Non registra nulla.** **ALTRO CODICE** per cercarne un altro, **CHIUDI** per uscire.

## 8. Impostazioni della postazione (solo per chi configura)

Nella schermata di accesso, **IMPOSTAZIONI** apre i valori di `C:\Esprit\Esprit.ini`, lo stesso
file usato dal programma originale.

- **Schermo intero** (prima opzione): SÌ/NO si applica subito. Il tasto **F11** lo attiva al volo.
- Le voci sì/no hanno i pulsanti SÌ/NO; le altre si toccano e si scrivono con la tastiera.
- Le password sono nascoste finché non premi **MOSTRA PASSWORD**.
- **SALVA** fa prima una copia di sicurezza del file (`Esprit.ini.bak-data-ora`), poi
  **RIAVVIA** fa ripartire l'app con i nuovi valori.

## 9. Fine turno e chiusura

- **FINE TURNO** (scheda OPERATORI) chiude il tuo turno e ti toglie dalle macchine; l'app torna
  alla schermata di accesso.
- Il pulsante in alto a destra (o la X della finestra) chiude l'app: prima chiude il turno degli
  operatori che hanno fatto l'accesso **su questo PC** e libera le loro macchine. Se c'è una fase
  in corso, l'app chiede se sospenderla; se qualcosa non riesce puoi **ANNULLARE** o **USCIRE
  COMUNQUE**.

## 10. Se qualcosa non funziona

- L'app non si apre: attendi un minuto senza cliccare. Se compare un messaggio di errore,
  invia a chi segue il programma il file `%LOCALAPPDATA%\RaccoltaDati\avvio.log`.
- "Modulo di salvataggio non trovato": i file `RaccoltaDati.exe` e `RdPonte.exe` devono stare
  nella stessa cartella.
- Taglie, lotti e consumi materiali: per ora si registrano con il programma originale.
