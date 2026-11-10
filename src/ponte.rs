//! Talks to RdPonte.exe, the helper that saves through the original program's own
//! business libraries. One request at a time, one line each way (see bridge/RdPonte.cs).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub enum Risposta {
    Ok(HashMap<String, String>),
    Errore(String),
    /// The original would show a yes/no box: answer and resend with `r<code>`.
    Domanda { code: String, testo: String },
    /// The close/suspend phase window (hours, minutes, operators).
    Finestra(HashMap<String, String>),
}

impl Risposta {
    pub fn get(&self, k: &str) -> String {
        match self {
            Risposta::Ok(m) | Risposta::Finestra(m) => m.get(k).cloned().unwrap_or_default(),
            _ => String::new(),
        }
    }
}

pub fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r")
}

pub fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn kv(fields: &[&str]) -> HashMap<String, String> {
    fields
        .iter()
        .filter_map(|f| f.split_once('=').map(|(k, v)| (k.to_string(), unesc(v))))
        .collect()
}

pub fn parse_reply(line: &str) -> Risposta {
    let parts: Vec<&str> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
    match parts.get(1).copied() {
        Some("OK") => Risposta::Ok(kv(&parts[2..])),
        Some("POPUP") => Risposta::Finestra(kv(&parts[2..])),
        Some("ASK") => Risposta::Domanda {
            code: parts.get(2).unwrap_or(&"").to_string(),
            testo: unesc(parts.get(3).unwrap_or(&"")),
        },
        Some("ERR") => Risposta::Errore(unesc(parts.get(2).unwrap_or(&""))),
        _ => Risposta::Errore(format!("Risposta non valida dal modulo di salvataggio: {line}")),
    }
}

struct Proc {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "0\texit");
        let _ = self.child.kill();
    }
}

pub struct Ponte {
    exe: PathBuf,
    lib_dir: String,
    init: Vec<(String, String)>,
    proc: Mutex<Option<Proc>>,
    pub sola_lettura: bool,
}

impl Ponte {
    pub fn new(exe: PathBuf, lib_dir: String, init: Vec<(String, String)>, sola_lettura: bool) -> Arc<Self> {
        Arc::new(Self { exe, lib_dir, init, proc: Mutex::new(None), sola_lettura })
    }

    pub fn exe_path() -> PathBuf {
        if let Ok(p) = std::env::var("RD_PONTE") {
            return PathBuf::from(p);
        }
        if let Ok(me) = std::env::current_exe() {
            let p = me.with_file_name("RdPonte.exe");
            if p.exists() {
                return p;
            }
        }
        PathBuf::from(env!("RD_PONTE_DEFAULT"))
    }

    pub fn exe_display(&self) -> String {
        self.exe.display().to_string()
    }

    pub fn available(&self) -> bool {
        self.exe.exists()
    }

    fn start(&self) -> Result<Proc, String> {
        let mut cmd = Command::new(&self.exe);
        cmd.arg(&self.lib_dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| format!("Modulo di salvataggio non avviato ({}): {e}", self.exe.display()))?;
        let stdin = child.stdin.take().ok_or("stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("stdout")?);
        let mut p = Proc { child, stdin, stdout, next_id: 1 };
        let init: Vec<(&str, String)> = self.init.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        match Self::send(&mut p, "init", &init)? {
            Risposta::Ok(_) => Ok(p),
            Risposta::Errore(e) => Err(format!("Connessione del modulo di salvataggio non riuscita: {e}")),
            other => Err(format!("{other:?}")),
        }
    }

    fn send(p: &mut Proc, cmd: &str, args: &[(&str, String)]) -> Result<Risposta, String> {
        let id = p.next_id;
        p.next_id += 1;
        let mut line = format!("{id}\t{cmd}");
        for (k, v) in args {
            line.push('\t');
            line.push_str(k);
            line.push('=');
            line.push_str(&esc(v));
        }
        writeln!(p.stdin, "{line}").map_err(|e| e.to_string())?;
        p.stdin.flush().map_err(|e| e.to_string())?;
        let mut reply = String::new();
        if p.stdout.read_line(&mut reply).map_err(|e| e.to_string())? == 0 {
            return Err("Il modulo di salvataggio si è chiuso".into());
        }
        Ok(parse_reply(&reply))
    }

    /// Blocking call; run it off the UI thread.
    pub fn call(&self, cmd: &str, args: &[(&str, String)]) -> Risposta {
        if self.sola_lettura && cmd != "operatore" && cmd != "vers_carica" && cmd != "ping" {
            return Risposta::Errore("Modalità sola lettura: nessuna registrazione è stata salvata.".into());
        }
        let mut guard = self.proc.lock().unwrap();
        for attempt in 0..2 {
            if guard.is_none() {
                match self.start() {
                    Ok(p) => *guard = Some(p),
                    Err(e) => return Risposta::Errore(e),
                }
            }
            match Self::send(guard.as_mut().unwrap(), cmd, args) {
                Ok(r) => return r,
                Err(e) => {
                    *guard = None;
                    if attempt == 1 {
                        return Risposta::Errore(e);
                    }
                }
            }
        }
        Risposta::Errore("Modulo di salvataggio non disponibile".into())
    }

    /// Starts the helper ahead of the first action so the first tap is quick.
    pub fn warm_up(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || {
            let _ = me.call("ping", &[]);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_round_trips() {
        let s = "a\tb\\c\nd";
        assert_eq!(unesc(&esc(s)), s);
    }

    #[test]
    fn replies() {
        match parse_reply("3\tOK\tmsg=REGISTRAZIONE EFFETTUATA\tx=a\\tb") {
            Risposta::Ok(m) => {
                assert_eq!(m["msg"], "REGISTRAZIONE EFFETTUATA");
                assert_eq!(m["x"], "a\tb");
            }
            _ => panic!(),
        }
        match parse_reply("4\tASK\t188\tChiudere?") {
            Risposta::Domanda { code, testo } => {
                assert_eq!(code, "188");
                assert_eq!(testo, "Chiudere?");
            }
            _ => panic!(),
        }
        assert!(matches!(parse_reply("5\tERR\tno"), Risposta::Errore(e) if e == "no"));
    }
}
