//! Reads the shared `Esprit.ini` the original program uses, so both apps talk
//! to the same database without copying any credentials into this one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_INI: &str = r"C:\Esprit\Esprit.ini";

#[derive(Clone, Debug, Default)]
pub struct Ini {
    pub path: PathBuf,
    sections: HashMap<String, HashMap<String, String>>,
}

impl Ini {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("Impossibile leggere {}: {e}", path.display()))?;
        // The file is written by Windows tools: UTF-8 when possible, else ANSI (Latin-1).
        let text = match String::from_utf8(bytes.clone()) {
            Ok(s) => s,
            Err(_) => bytes.iter().map(|&b| b as char).collect(),
        };
        Ok(Self::parse(path, &text))
    }

    pub fn parse(path: &Path, text: &str) -> Self {
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current = String::new();
        for raw in text.lines() {
            let line = raw.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                current = line[1..line.len() - 1].trim().to_lowercase();
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                sections
                    .entry(current.clone())
                    .or_default()
                    .insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        Self { path: path.to_path_buf(), sections }
    }

    /// Same as `Ini.Read(section, key)`: empty string when missing.
    pub fn get(&self, section: &str, key: &str) -> String {
        self.sections
            .get(&section.to_lowercase())
            .and_then(|s| s.get(&key.to_lowercase()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_or(&self, section: &str, key: &str, default: &str) -> String {
        let v = self.get(section, key);
        if v.is_empty() { default.to_string() } else { v }
    }
}

/// Where the settings file lives: `--ini <path>`, `RACCOLTA_DATI_INI`, else the original's path.
pub fn ini_path(args: &[String]) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--ini") {
        if let Some(p) = args.get(i + 1) {
            return PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("RACCOLTA_DATI_INI") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from(DEFAULT_INI)
}

/// The `[Raccolta dati]` section, read the same way as the original's Program.Main.
#[derive(Clone, Debug)]
pub struct Postazione {
    pub commesse_per_blocco: usize,
    pub macchine: Vec<String>,
    pub operatori: Vec<String>,
    pub touch: bool,
    pub display_ridotto: bool,
    pub ins_parziale: bool,
    pub auto_refresh_commesse: bool,
    pub max_len_note: usize,
    pub registra_pesate: bool,
    pub usa_badge: bool,
    pub torna_scheda_macchine: bool,
    pub con_str_macchine: String,
    pub articolo_label: String,
    /// SCHERMO_INTERO=Y: full screen (this app only; the original does not read it).
    pub schermo_intero: bool,
}

impl Postazione {
    pub fn from_ini(ini: &Ini) -> Self {
        let s = "Raccolta dati";
        let list = |k: &str| ini.get(s, k).split(',').map(|x| x.trim().to_string()).collect::<Vec<_>>();
        Self {
            // int.Parse(BloccoCommesse): the original fails without it; 4 keeps the app usable.
            commesse_per_blocco: ini.get(s, "BloccoCommesse").trim().parse().ok().filter(|n| *n > 0).unwrap_or(4),
            macchine: list("MACCHINE").into_iter().filter(|x| !x.is_empty()).collect(),
            operatori: list("OPERATORI"),
            touch: ini.get(s, "TOUCH") == "Y",
            display_ridotto: ini.get(s, "DISPLAYRIDOTTO") == "Y",
            ins_parziale: ini.get(s, "InsParziale") == "S",
            auto_refresh_commesse: ini.get(s, "AutoRefreshCommesse") == "Y",
            max_len_note: ini.get(s, "MaxLengthNoteCommessa").trim().parse().unwrap_or(80),
            registra_pesate: ini.get(s, "REGISTRA_PESATE") == "Y",
            usa_badge: ini.get(s, "USA_BADGE_OPERATORE") == "Y",
            torna_scheda_macchine: ini.get(s, "TORNA_SCHEDA_MACCHINE_DOPO_OPERAZIONI") == "Y",
            con_str_macchine: ini.get(s, "CONSTRSQLINPUTDRIVER"),
            articolo_label: ini.get(s, "ArticoloLabel"),
            schermo_intero: ini.get(s, "SCHERMO_INTERO") == "Y",
        }
    }

    /// `CodOperatoriTurno[0] != ""`: only these operators are shown.
    pub fn operatore_ammesso(&self, cod: &str) -> bool {
        self.operatori.first().map(|f| f.is_empty()).unwrap_or(true) || self.operatori.iter().any(|o| o == cod)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sections_case_insensitively() {
        let ini = Ini::parse(
            Path::new("x.ini"),
            "[Connect]\n;comment\nSERVERSQL=SRV\nDATABASESQL = DB\n\n[SigmaWebApi]\nAPIURL=http://h/api\n",
        );
        assert_eq!(ini.get("Connect", "SERVERSQL"), "SRV");
        assert_eq!(ini.get("connect", "databasesql"), "DB");
        assert_eq!(ini.get("SigmaWebApi", "APIURL"), "http://h/api");
        assert_eq!(ini.get("Connect", "missing"), "");
        assert_eq!(ini.get_or("Connect", "LocalLang", "IT"), "IT");
    }
}
