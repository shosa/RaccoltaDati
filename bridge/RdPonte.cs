// RdPonte: runs the original program's save actions with the original's own
// business libraries (RD.Logic, RD.Dal.MSSql, Esprit.*), so every write lands in
// the database exactly as "Raccolta dati.exe" would write it.
//
// The new window (Rust) talks to this process over stdin/stdout, one line per
// message:  request  "<id>\t<cmd>\t<key>=<value>\t..."
//           replies  "<id>\tOK\t<key>=<value>..."  |  "<id>\tERR\t<text>"
//                    "<id>\tASK\t<code>\t<question>"  (answer, then resend the
//                    same request with  r<code>=S|N|A)
//                    "<id>\tPOPUP\t<key>=<value>..." (closing-phase window needed)
// Values escape "\" "\t" "\n" "\r" as \\ \t \n \r.
//
// Compiled with the .NET Framework C# compiler (C# 5) by build.rs; the library
// folder is passed as the first argument and resolved at run time.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;

static class Program
{
    static string LibDir;

    [STAThread]
    static int Main(string[] args)
    {
        LibDir = args.Length > 0 ? args[0] : @"C:\Esprit\Raccolta Dati";
        AppDomain.CurrentDomain.AssemblyResolve += Resolve;
        return Ponte.Run();
    }

    static Assembly Resolve(object sender, ResolveEventArgs e)
    {
        string name = new AssemblyName(e.Name).Name;
        foreach (string ext in new[] { ".dll", ".exe" })
        {
            string p = Path.Combine(LibDir, name + ext);
            if (File.Exists(p)) return Assembly.LoadFrom(p);
        }
        return null;
    }
}

// --------------------------------------------------------------------------
// Protocol

class AskException : Exception
{
    public string Code;
    public AskException(string code, string text) : base(text) { Code = code; }
}

class PopupException : Exception
{
    public Dictionary<string, string> Data2;
    public PopupException(Dictionary<string, string> d) : base("popup") { Data2 = d; }
}

class UserError : Exception
{
    public UserError(string text) : base(text) { }
}

class Req
{
    public string Id;
    public string Cmd;
    public Dictionary<string, string> Args = new Dictionary<string, string>();

    public string S(string k)
    {
        string v;
        return Args.TryGetValue(k, out v) ? v : "";
    }

    public int I(string k) { return int.Parse(S(k), CultureInfo.InvariantCulture); }

    public decimal D(string k)
    {
        string v = S(k).Replace(',', '.');
        return v == "" ? 0m : decimal.Parse(v, CultureInfo.InvariantCulture);
    }

    public bool Has(string k) { return Args.ContainsKey(k) && Args[k] != ""; }
}

static partial class Ponte
{
    static TextWriter Out;

    public static string Esc(string s)
    {
        if (s == null) return "";
        return s.Replace("\\", "\\\\").Replace("\t", "\\t").Replace("\n", "\\n").Replace("\r", "\\r");
    }

    static string Unesc(string s)
    {
        var sb = new StringBuilder();
        for (int i = 0; i < s.Length; i++)
        {
            char c = s[i];
            if (c == '\\' && i + 1 < s.Length)
            {
                char n = s[++i];
                sb.Append(n == 't' ? '\t' : n == 'n' ? '\n' : n == 'r' ? '\r' : n);
            }
            else sb.Append(c);
        }
        return sb.ToString();
    }

    static Req Parse(string line)
    {
        string[] parts = line.Split('\t');
        var r = new Req { Id = parts[0], Cmd = parts.Length > 1 ? parts[1] : "" };
        for (int i = 2; i < parts.Length; i++)
        {
            int eq = parts[i].IndexOf('=');
            if (eq > 0) r.Args[parts[i].Substring(0, eq)] = Unesc(parts[i].Substring(eq + 1));
        }
        return r;
    }

    static void Send(string id, string kind, IEnumerable<string> fields)
    {
        var sb = new StringBuilder();
        sb.Append(id).Append('\t').Append(kind);
        foreach (string f in fields) sb.Append('\t').Append(f);
        lock (Out) { Out.WriteLine(sb.ToString()); Out.Flush(); }
    }

    static IEnumerable<string> Kv(Dictionary<string, string> d)
    {
        return d.Select(p => p.Key + "=" + Esc(p.Value));
    }

    public static int Run()
    {
        var stdout = Console.OpenStandardOutput();
        Out = new StreamWriter(stdout, new UTF8Encoding(false)) { AutoFlush = true };
        // Libraries must never write on our protocol channel.
        Console.SetOut(new StreamWriter(Stream.Null));
        var input = new StreamReader(Console.OpenStandardInput(), new UTF8Encoding(false));
        string line;
        while ((line = input.ReadLine()) != null)
        {
            if (line.Trim() == "") continue;
            Req r = Parse(line);
            if (r.Cmd == "exit") { Send(r.Id, "OK", new string[0]); break; }
            try
            {
                Esprit.Logic.Configurazioni.ExMessage = "";
                Dictionary<string, string> res = Dispatch(r);
                Send(r.Id, "OK", Kv(res ?? new Dictionary<string, string>()));
            }
            catch (AskException a)
            {
                Send(r.Id, "ASK", new[] { a.Code, Esc(a.Message) });
            }
            catch (PopupException p)
            {
                Send(r.Id, "POPUP", Kv(p.Data2));
            }
            catch (Exception ex)
            {
                Send(r.Id, "ERR", new[] { Esc(Describe(ex)) });
            }
        }
        return 0;
    }

    // Same text the original shows: message number -> text from the database.
    static string Describe(Exception ex)
    {
        if (ex is TargetInvocationException && ex.InnerException != null) ex = ex.InnerException;
        if (ex is UserError) return ex.Message;
        int code;
        string extra = Esprit.Logic.Configurazioni.ExMessage ?? "";
        if (int.TryParse(ex.Message, out code))
        {
            return (Testo(code) + " " + extra).Trim();
        }
        string msg = ex.Message;
        if (ex.InnerException != null && ex.InnerException.Message != ex.Message) msg += " (" + ex.InnerException.Message + ")";
        return msg;
    }

    public static string Testo(int code)
    {
        try { return Esprit.WinForm.EspMessageBox.getTesto(code); }
        catch (Exception) { return "Messaggio " + code; }
    }

    public static string Testo(string code) { return Testo(int.Parse(code)); }

    // A yes/no question the original asks with a message box.
    // Returns the answer ("S", "N" or "A" for cancel); asks the window when missing.
    static string Chiedi(Req r, string code, string extra = "")
    {
        string a = r.S("r" + code);
        if (a != "") return a;
        throw new AskException(code, (Testo(code) + " " + (extra ?? "")).Trim());
    }

    static bool Si(Req r, string code, string extra = "") { return Chiedi(r, code, extra) == "S"; }

    static Dictionary<string, string> Ok(string msgCode = null)
    {
        var d = new Dictionary<string, string>();
        if (msgCode != null) d["msg"] = Testo(msgCode);
        return d;
    }
}
