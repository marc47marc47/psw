use chrono::{DateTime, Local, TimeZone};
use clap::Parser;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;
use sysinfo::{
    MINIMUM_CPU_UPDATE_INTERVAL, Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System,
    Users,
};

#[derive(Parser)]
#[command(author, version, about = "ps-like process listing for Windows", long_about = None)]
struct Cli {
    /// Select all processes (default: only the current user's)
    #[arg(short = 'e', long = "all")]
    all: bool,

    /// Long format: adds process status and resident memory (RSS)
    #[arg(short = 'l', long = "long")]
    long: bool,

    /// Full format: adds UID/PPID/%CPU/STIME and shows the full command line
    #[arg(short = 'f', long = "full")]
    full: bool,
}

impl Cli {
    /// `%CPU` is a rate, so it needs two samples; only pay that delay when it is displayed.
    fn wants_cpu_percent(&self) -> bool {
        self.all || self.long || self.full
    }

    fn wants_full_cmdline(&self) -> bool {
        self.long || self.full
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Stat,
    Uid,
    Pid,
    Ppid,
    Cpu,
    Rss,
    Stime,
    Time,
    Cmd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Right,
}

impl Column {
    fn header(self) -> &'static str {
        match self {
            Column::Stat => "STAT",
            Column::Uid => "UID",
            Column::Pid => "PID",
            Column::Ppid => "PPID",
            Column::Cpu => "%CPU",
            Column::Rss => "RSS",
            Column::Stime => "STIME",
            Column::Time => "TIME",
            Column::Cmd => "CMD",
        }
    }

    fn align(self) -> Align {
        match self {
            Column::Pid | Column::Ppid | Column::Cpu | Column::Rss => Align::Right,
            _ => Align::Left,
        }
    }
}

/// Only columns whose value the OS actually provides are emitted (no placeholder columns).
fn columns_for(cli: &Cli) -> Vec<Column> {
    use Column::*;
    if cli.long {
        vec![Stat, Uid, Pid, Ppid, Cpu, Rss, Stime, Time, Cmd]
    } else if cli.full || cli.all {
        vec![Uid, Pid, Ppid, Cpu, Stime, Time, Cmd]
    } else {
        vec![Pid, Time, Cmd]
    }
}

/// `HH:MM:SS` from milliseconds (hours are not wrapped at 24).
fn format_cpu_time(millis: u64) -> String {
    let total = millis / 1000;
    format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60)
}

/// `HH:MM` for processes started today, `MonDD` otherwise (like `ps`).
fn format_start_time(start_epoch_secs: u64, now: DateTime<Local>) -> String {
    let Ok(secs) = i64::try_from(start_epoch_secs) else {
        return "-".to_string();
    };
    match Local.timestamp_opt(secs, 0).single() {
        Some(t) if t.date_naive() == now.date_naive() => t.format("%H:%M").to_string(),
        Some(t) => t.format("%b%d").to_string(),
        None => "-".to_string(),
    }
}

fn format_cmd(process: &Process, full: bool) -> String {
    let name = process.name().to_string_lossy();
    if !full {
        return name.into_owned();
    }
    if process.cmd().is_empty() {
        return format!("[{name}]");
    }
    process
        .cmd()
        .iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

struct RowContext<'a> {
    users: &'a Users,
    now: DateTime<Local>,
    full_cmdline: bool,
}

fn cell(col: Column, pid: &Pid, p: &Process, ctx: &RowContext) -> String {
    match col {
        Column::Stat => p.status().to_string(),
        Column::Uid => match p.user_id() {
            Some(uid) => ctx
                .users
                .get_user_by_id(uid)
                .map(|u| u.name().to_string())
                .unwrap_or_else(|| uid.to_string()),
            None => "-".to_string(),
        },
        Column::Pid => pid.to_string(),
        Column::Ppid => p.parent().map_or_else(|| "-".to_string(), |pp| pp.to_string()),
        Column::Cpu => format!("{:.1}", p.cpu_usage()),
        Column::Rss => (p.memory() / 1024).to_string(),
        Column::Stime => format_start_time(p.start_time(), ctx.now),
        Column::Time => format_cpu_time(p.accumulated_cpu_time()),
        Column::Cmd => format_cmd(p, ctx.full_cmdline),
    }
}

/// Renders a header + rows with per-column widths; the last column is never padded.
fn render_table(cols: &[Column], rows: &[Vec<String>]) -> Vec<String> {
    let mut widths: Vec<usize> = cols.iter().map(|c| c.header().chars().count()).collect();
    for row in rows {
        for (w, v) in widths.iter_mut().zip(row) {
            *w = (*w).max(v.chars().count());
        }
    }
    let fmt_line = |cells: &mut dyn Iterator<Item = &str>| -> String {
        let mut line = String::new();
        let last = cols.len() - 1;
        for (i, v) in cells.enumerate() {
            if i > 0 {
                line.push(' ');
            }
            match (cols[i].align(), i == last) {
                (Align::Right, _) => line.push_str(&format!("{v:>w$}", w = widths[i])),
                (Align::Left, true) => line.push_str(v),
                (Align::Left, false) => line.push_str(&format!("{v:<w$}", w = widths[i])),
            }
        }
        line
    };
    let mut out = Vec::with_capacity(rows.len() + 1);
    out.push(fmt_line(&mut cols.iter().map(|c| c.header())));
    for row in rows {
        out.push(fmt_line(&mut row.iter().map(String::as_str)));
    }
    out
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let refresh_kind = ProcessRefreshKind::everything().without_tasks();
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);
    if cli.wants_cpu_percent() {
        // cpu_usage() is a delta between two refreshes; a single refresh always yields 0.
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);
    }

    let current_uid = if cli.all {
        None
    } else {
        let uid = sysinfo::get_current_pid()
            .ok()
            .and_then(|pid| sys.process(pid))
            .and_then(|p| p.user_id());
        match uid {
            Some(uid) => Some(uid.clone()),
            None => {
                eprintln!("psw: cannot determine the current user; use -e to list all processes");
                return ExitCode::FAILURE;
            }
        }
    };

    let users = Users::new_with_refreshed_list();
    let ctx = RowContext {
        users: &users,
        now: Local::now(),
        full_cmdline: cli.wants_full_cmdline(),
    };
    let cols = columns_for(&cli);

    let mut procs: Vec<(&Pid, &Process)> = sys
        .processes()
        .iter()
        .filter(|(_, p)| current_uid.as_ref().is_none_or(|uid| p.user_id() == Some(uid)))
        .collect();
    procs.sort_by_key(|(pid, _)| **pid);

    let rows: Vec<Vec<String>> = procs
        .iter()
        .map(|(pid, p)| cols.iter().map(|&c| cell(c, pid, p, &ctx)).collect())
        .collect();

    let mut out = BufWriter::new(io::stdout().lock());
    for line in render_table(&cols, &rows) {
        if let Err(e) = writeln!(out, "{line}") {
            // A closed pipe (e.g. `psw | head`) is a normal way to stop.
            return if e.kind() == io::ErrorKind::BrokenPipe {
                ExitCode::SUCCESS
            } else {
                eprintln!("psw: write error: {e}");
                ExitCode::FAILURE
            };
        }
    }
    match out.flush() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("psw: write error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(e: bool, l: bool, f: bool) -> Cli {
        Cli { all: e, long: l, full: f }
    }

    #[test]
    fn cpu_time_formatting() {
        assert_eq!(format_cpu_time(0), "00:00:00");
        assert_eq!(format_cpu_time(999), "00:00:00");
        assert_eq!(format_cpu_time(3_725_000), "01:02:05");
        assert_eq!(format_cpu_time(100 * 3_600_000), "100:00:00");
    }

    #[test]
    fn start_time_today_vs_older() {
        let now = Local.with_ymd_and_hms(2026, 3, 15, 12, 0, 0).unwrap();
        let today = Local.with_ymd_and_hms(2026, 3, 15, 8, 5, 0).unwrap().timestamp() as u64;
        let older = Local.with_ymd_and_hms(2026, 3, 1, 8, 5, 0).unwrap().timestamp() as u64;
        assert_eq!(format_start_time(today, now), "08:05");
        assert_eq!(format_start_time(older, now), "Mar01");
        assert_eq!(format_start_time(u64::MAX, now), "-");
    }

    #[test]
    fn column_selection() {
        use Column::*;
        assert_eq!(columns_for(&cli(false, false, false)), vec![Pid, Time, Cmd]);
        assert_eq!(columns_for(&cli(true, false, false)), columns_for(&cli(false, false, true)));
        assert_eq!(columns_for(&cli(false, true, false)).len(), 9);
        assert_eq!(columns_for(&cli(true, true, true)), columns_for(&cli(false, true, false)));
        assert!(!cli(false, false, false).wants_cpu_percent());
        assert!(cli(true, false, false).wants_cpu_percent());
        assert!(!cli(true, false, false).wants_full_cmdline());
        assert!(cli(false, true, false).wants_full_cmdline());
    }

    #[test]
    fn table_alignment() {
        let cols = [Column::Pid, Column::Time, Column::Cmd];
        let rows = vec![
            vec!["4".to_string(), "00:00:01".to_string(), "System".to_string()],
            vec!["12345".to_string(), "01:00:00".to_string(), "a b c".to_string()],
        ];
        assert_eq!(
            render_table(&cols, &rows),
            vec![
                "  PID TIME     CMD",
                "    4 00:00:01 System",
                "12345 01:00:00 a b c",
            ]
        );
    }
}
