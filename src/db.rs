//! SQL Server access. Every query runs on a background Tokio runtime so the
//! window never freezes; connections are pooled instead of opened per query.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use eframe::egui;
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, Query};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

pub type SqlClient = Client<Compat<TcpStream>>;

#[derive(Clone, Debug, Default)]
pub struct ConnParams {
    pub server: String,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl ConnParams {
    /// "host", "host,port", "tcp:host,port" or "host\instance" (instance ignored, default port).
    pub fn host_port(&self) -> (String, u16) {
        let s = self.server.trim().trim_start_matches("tcp:");
        let (host, port) = match s.split_once(',') {
            Some((h, p)) => (h.to_string(), p.trim().parse().unwrap_or(1433)),
            None => (s.to_string(), 1433),
        };
        let host = host.split('\\').next().unwrap_or("").to_string();
        let host = if host == "." || host.eq_ignore_ascii_case("(local)") { "localhost".into() } else { host };
        (host, port)
    }

    fn config(&self, encryption: EncryptionLevel) -> Config {
        let (host, port) = self.host_port();
        let mut c = Config::new();
        c.host(host);
        c.port(port);
        c.database(&self.database);
        c.authentication(AuthMethod::sql_server(&self.user, &self.password));
        c.trust_cert(); // TrustServerCertificate=True, as in the original connection string
        c.encryption(encryption);
        c.application_name("Raccolta Dati");
        c
    }

    pub fn label(&self) -> String {
        format!("{} · {}", self.server, self.database)
    }
}

/// Connection timeout of the original: RD.Logic SQLServer.GetConnectionString (0x1e6a8) sets
/// none, so Microsoft.Data.SqlClient's default Connect Timeout = 15 s applies (getNewConn 0x1e74f).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

async fn connect_with(p: &ConnParams, enc: EncryptionLevel) -> Result<SqlClient, String> {
    let cfg = p.config(enc);
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(cfg.get_addr()))
        .await
        .map_err(|_| format!("Il server {} non risponde", p.server))?
        .map_err(|e| format!("Connessione a {} non riuscita: {e}", p.server))?;
    tcp.set_nodelay(true).ok();
    match Client::connect(cfg, tcp.compat_write()).await {
        Ok(c) => Ok(c),
        Err(tiberius::error::Error::Routing { host, port }) => {
            let mut cfg = p.config(enc);
            cfg.host(&host);
            cfg.port(port);
            let tcp = TcpStream::connect(cfg.get_addr()).await.map_err(|e| e.to_string())?;
            tcp.set_nodelay(true).ok();
            Client::connect(cfg, tcp.compat_write()).await.map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

pub async fn connect(p: &ConnParams) -> Result<SqlClient, String> {
    match connect_with(p, EncryptionLevel::Required).await {
        Ok(c) => Ok(c),
        // Older servers without TLS: same as the original's driver falling back.
        Err(e) if e.to_lowercase().contains("tls") || e.to_lowercase().contains("encrypt") => {
            connect_with(p, EncryptionLevel::NotSupported).await
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Values

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Text(String),
    Int(i64),
    Num(f64),
    Bool(bool),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    Time(NaiveTime),
}

impl Cell {
    pub fn text(&self) -> String {
        match self {
            Cell::Null => String::new(),
            Cell::Text(s) => s.clone(),
            Cell::Int(i) => i.to_string(),
            Cell::Num(f) => fmt_num(*f),
            Cell::Bool(b) => if *b { "Sì".into() } else { "No".into() },
            Cell::Date(d) => d.format("%d/%m/%Y").to_string(),
            Cell::DateTime(d) => d.format("%d/%m/%Y %H:%M").to_string(),
            Cell::Time(t) => t.format("%H:%M").to_string(),
        }
    }

    pub fn f64(&self) -> Option<f64> {
        match self {
            Cell::Int(i) => Some(*i as f64),
            Cell::Num(f) => Some(*f),
            Cell::Text(s) => s.trim().replace(',', ".").parse().ok(),
            Cell::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn i64(&self) -> Option<i64> {
        self.f64().map(|f| f as i64)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Cell::Int(_) | Cell::Num(_))
    }

    /// Ordering used when a column header is clicked.
    pub fn cmp_key(&self, other: &Cell) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Cell::Null, Cell::Null) => Ordering::Equal,
            (Cell::Null, _) => Ordering::Less,
            (_, Cell::Null) => Ordering::Greater,
            (Cell::Date(a), Cell::Date(b)) => a.cmp(b),
            (Cell::DateTime(a), Cell::DateTime(b)) => a.cmp(b),
            (a, b) if a.is_numeric() && b.is_numeric() => {
                a.f64().partial_cmp(&b.f64()).unwrap_or(Ordering::Equal)
            }
            (a, b) => a.text().to_lowercase().cmp(&b.text().to_lowercase()),
        }
    }
}

/// Italian number text: decimal comma, thousands dot, up to 3 decimals.
pub fn fmt_num(f: f64) -> String {
    if !f.is_finite() {
        return String::new();
    }
    let neg = f < 0.0;
    let rounded = (f.abs() * 1000.0).round() / 1000.0;
    let int = rounded.trunc() as u64;
    let mut frac = format!("{:.3}", rounded.fract());
    frac = frac.trim_start_matches('0').trim_start_matches('.').trim_end_matches('0').to_string();
    let digits = int.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let mut out = if neg && (int > 0 || !frac.is_empty()) { format!("-{grouped}") } else { grouped };
    if !frac.is_empty() {
        out.push(',');
        out.push_str(&frac);
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Table {
    #[allow(dead_code)]
    pub cols: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
    pub elapsed_ms: u128,
    index: HashMap<String, usize>,
}

impl Table {
    pub fn new(cols: Vec<String>, rows: Vec<Vec<Cell>>) -> Self {
        let index = cols.iter().enumerate().map(|(i, c)| (c.to_uppercase(), i)).collect();
        Self { cols, rows, elapsed_ms: 0, index }
    }

    pub fn col(&self, name: &str) -> Option<usize> {
        self.index.get(&name.to_uppercase()).copied()
    }

    pub fn get<'a>(&'a self, row: &'a [Cell], name: &str) -> &'a Cell {
        static NULL: Cell = Cell::Null;
        self.col(name).and_then(|i| row.get(i)).unwrap_or(&NULL)
    }

    pub fn s(&self, row: &[Cell], name: &str) -> String {
        self.get(row, name).text()
    }
}

// ---------------------------------------------------------------------------
// Statements

#[derive(Clone, Debug)]
pub enum Param {
    Str(String),
    Int(i32),
    Float(f64),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    Null,
}

impl From<&str> for Param {
    fn from(s: &str) -> Self { Param::Str(s.to_string()) }
}
impl From<String> for Param {
    fn from(s: String) -> Self { Param::Str(s) }
}
impl From<&String> for Param {
    fn from(s: &String) -> Self { Param::Str(s.clone()) }
}
impl From<i32> for Param {
    fn from(i: i32) -> Self { Param::Int(i) }
}
impl From<f64> for Param {
    fn from(f: f64) -> Self { Param::Float(f) }
}
impl From<NaiveDate> for Param {
    fn from(d: NaiveDate) -> Self { Param::Date(d) }
}
impl From<NaiveDateTime> for Param {
    fn from(d: NaiveDateTime) -> Self { Param::DateTime(d) }
}

/// SQL text with numbered parameters (`@P1`, `@P2`, ...). Values are never pasted into the text.
#[derive(Clone, Debug, Default)]
pub struct Sql {
    pub text: String,
    pub params: Vec<Param>,
}

impl Sql {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), params: Vec::new() }
    }

    /// Adds a parameter and returns its placeholder.
    pub fn arg(&mut self, p: impl Into<Param>) -> String {
        self.params.push(p.into());
        format!("@P{}", self.params.len())
    }

    pub fn push(&mut self, s: &str) -> &mut Self {
        self.text.push_str(s);
        self
    }

    /// Appends `fragment` with every `?` replaced by the next value.
    pub fn and(&mut self, fragment: &str, values: Vec<Param>) -> &mut Self {
        let mut out = String::new();
        let mut it = values.into_iter();
        for ch in fragment.chars() {
            if ch == '?' {
                let v = it.next().unwrap_or(Param::Null);
                out.push_str(&self.arg(v));
            } else {
                out.push(ch);
            }
        }
        self.text.push(' ');
        self.text.push_str(&out);
        self.text.push(' ');
        self
    }
}

fn bind_all<'a>(sql: &'a Sql) -> Query<'a> {
    let mut q = Query::new(sql.text.as_str());
    for p in &sql.params {
        match p {
            Param::Str(s) => q.bind(s.as_str()),
            Param::Int(i) => q.bind(*i),
            Param::Float(f) => q.bind(*f),
            Param::Date(d) => q.bind(*d),
            Param::DateTime(d) => q.bind(*d),
            Param::Null => q.bind(Option::<&str>::None),
        }
    }
    q
}

fn to_cell(row: &tiberius::Row, idx: usize, data: &ColumnData<'static>) -> Cell {
    match data {
        ColumnData::U8(v) => v.map(|x| Cell::Int(x as i64)).unwrap_or(Cell::Null),
        ColumnData::I16(v) => v.map(|x| Cell::Int(x as i64)).unwrap_or(Cell::Null),
        ColumnData::I32(v) => v.map(|x| Cell::Int(x as i64)).unwrap_or(Cell::Null),
        ColumnData::I64(v) => v.map(Cell::Int).unwrap_or(Cell::Null),
        ColumnData::F32(v) => v.map(|x| Cell::Num(x as f64)).unwrap_or(Cell::Null),
        ColumnData::F64(v) => v.map(Cell::Num).unwrap_or(Cell::Null),
        ColumnData::Bit(v) => v.map(Cell::Bool).unwrap_or(Cell::Null),
        ColumnData::String(v) => v
            .as_ref()
            .map(|s| Cell::Text(s.trim_end().to_string()))
            .unwrap_or(Cell::Null),
        ColumnData::Numeric(v) => v
            .map(|n| {
                let f = n.value() as f64 / 10f64.powi(n.scale() as i32);
                if n.scale() == 0 { Cell::Int(n.value() as i64) } else { Cell::Num(f) }
            })
            .unwrap_or(Cell::Null),
        ColumnData::Guid(v) => v.map(|g| Cell::Text(g.to_string())).unwrap_or(Cell::Null),
        ColumnData::Binary(v) => v.as_ref().map(|b| Cell::Text(format!("<{} byte>", b.len()))).unwrap_or(Cell::Null),
        ColumnData::Xml(v) => v.as_ref().map(|x| Cell::Text(x.to_string())).unwrap_or(Cell::Null),
        ColumnData::Date(_) => row
            .try_get::<NaiveDate, _>(idx)
            .ok()
            .flatten()
            .map(Cell::Date)
            .unwrap_or(Cell::Null),
        ColumnData::Time(_) => row
            .try_get::<NaiveTime, _>(idx)
            .ok()
            .flatten()
            .map(Cell::Time)
            .unwrap_or(Cell::Null),
        _ => row
            .try_get::<NaiveDateTime, _>(idx)
            .ok()
            .flatten()
            .map(Cell::DateTime)
            .unwrap_or(Cell::Null),
    }
}

async fn run_query(client: &mut SqlClient, sql: &Sql) -> Result<Table, String> {
    let q = bind_all(sql);
    let mut stream = q.query(client).await.map_err(|e| e.to_string())?;
    let cols: Vec<String> = stream
        .columns()
        .await
        .map_err(|e| e.to_string())?
        .map(|c| c.iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let rows = stream.into_first_result().await.map_err(|e| e.to_string())?;
    let rows = rows
        .iter()
        .map(|r| r.cells().enumerate().map(|(i, (_, d))| to_cell(r, i, d)).collect())
        .collect();
    Ok(Table::new(cols, rows))
}

async fn run_exec(client: &mut SqlClient, sql: &Sql) -> Result<u64, String> {
    let q = bind_all(sql);
    let res = q.execute(client).await.map_err(|e| e.to_string())?;
    Ok(res.total())
}

// ---------------------------------------------------------------------------
// Handle + pool

struct Inner {
    params: ConnParams,
    pool: tokio::sync::Mutex<Vec<SqlClient>>,
    rt: tokio::runtime::Handle,
    ctx: egui::Context,
}

#[derive(Clone)]
pub struct Db {
    inner: Arc<Inner>,
}

impl Db {
    pub fn new(params: ConnParams, rt: tokio::runtime::Handle, ctx: egui::Context) -> Self {
        Self { inner: Arc::new(Inner { params, pool: tokio::sync::Mutex::new(Vec::new()), rt, ctx }) }
    }

    pub fn params(&self) -> &ConnParams {
        &self.inner.params
    }

    pub fn ctx(&self) -> egui::Context {
        self.inner.ctx.clone()
    }

    async fn take(&self) -> Result<(SqlClient, bool), String> {
        if let Some(c) = self.inner.pool.lock().await.pop() {
            return Ok((c, true));
        }
        Ok((connect(&self.inner.params).await?, false))
    }

    async fn give(&self, c: SqlClient) {
        let mut pool = self.inner.pool.lock().await;
        if pool.len() < 6 {
            pool.push(c);
        }
    }

    pub async fn query(&self, sql: Sql) -> Result<Table, String> {
        let started = Instant::now();
        let (mut c, pooled) = self.take().await?;
        let mut res = run_query(&mut c, &sql).await;
        if res.is_err() && pooled {
            // A pooled connection may have been closed by the server: retry once on a fresh one.
            c = connect(&self.inner.params).await?;
            res = run_query(&mut c, &sql).await;
        }
        if res.is_ok() {
            self.give(c).await;
        }
        res.map(|mut t| {
            t.elapsed_ms = started.elapsed().as_millis();
            t
        })
    }

    pub async fn execute(&self, sql: Sql) -> Result<u64, String> {
        let (mut c, _) = self.take().await?;
        let res = run_exec(&mut c, &sql).await;
        if res.is_ok() {
            self.give(c).await;
        }
        res
    }

    /// Runs `fut` in the background; the window repaints when it finishes.
    pub fn spawn<T: Send + 'static>(
        &self,
        fut: impl Future<Output = Result<T, String>> + Send + 'static,
    ) -> Task<T> {
        spawn_on(&self.inner.rt, &self.inner.ctx, fut)
    }
}

pub fn spawn_on<T: Send + 'static>(
    rt: &tokio::runtime::Handle,
    ctx: &egui::Context,
    fut: impl Future<Output = Result<T, String>> + Send + 'static,
) -> Task<T> {
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let ctx = ctx.clone();
    rt.spawn(async move {
        let r = fut.await;
        *out.lock().unwrap() = Some(r);
        ctx.request_repaint();
    });
    Task { slot, started: Instant::now() }
}

pub struct Task<T> {
    slot: Arc<Mutex<Option<Result<T, String>>>>,
    pub started: Instant,
}

impl<T> Task<T> {
    pub fn poll(&self) -> Option<Result<T, String>> {
        self.slot.lock().unwrap().take()
    }
}

/// A value that loads in the background.
pub struct Loader<T> {
    task: Option<Task<T>>,
    pub value: Option<T>,
    pub error: Option<String>,
    pub ms: u128,
}

impl<T> Default for Loader<T> {
    fn default() -> Self {
        Self { task: None, value: None, error: None, ms: 0 }
    }
}

impl<T> Loader<T> {
    pub fn start(&mut self, task: Task<T>) {
        self.task = Some(task);
        self.error = None;
    }

    pub fn loading(&self) -> bool {
        self.task.is_some()
    }

    /// Returns true when a result just arrived.
    pub fn poll(&mut self) -> bool {
        let Some(t) = &self.task else { return false };
        let Some(r) = t.poll() else { return false };
        self.ms = t.started.elapsed().as_millis();
        self.task = None;
        match r {
            Ok(v) => {
                self.value = Some(v);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_italian() {
        assert_eq!(fmt_num(1234567.5), "1.234.567,5");
        assert_eq!(fmt_num(12.0), "12");
        assert_eq!(fmt_num(-0.25), "-0,25");
        assert_eq!(fmt_num(0.0), "0");
    }

    #[test]
    fn placeholders() {
        let mut s = Sql::new("SELECT 1 WHERE 1=1");
        s.and("AND A >= ? AND B <= ?", vec!["x".into(), 3.into()]);
        assert!(s.text.contains("A >= @P1 AND B <= @P2"));
        assert_eq!(s.params.len(), 2);
    }

    #[test]
    fn server_parsing() {
        let p = ConnParams { server: "tcp:SRV,1500".into(), ..Default::default() };
        assert_eq!(p.host_port(), ("SRV".into(), 1500));
        let p = ConnParams { server: r"SRV\SQLEXPRESS".into(), ..Default::default() };
        assert_eq!(p.host_port(), ("SRV".into(), 1433));
    }
}
