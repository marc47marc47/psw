use chrono::{Local, TimeZone};
use clap::Parser;
use std::io::{self, Write};
use sysinfo::{System, SystemExt, UserExt, ProcessExt, Pid};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    e: bool,

    #[arg(short, long)]
    l: bool,

    #[arg(short, long)]
    f: bool,
}

fn main() {
    let cli = Cli::parse();

    let mut sys = System::new_all();
    sys.refresh_all();

    let users = sys.users();

    if cli.l && cli.f {
        if writeln!(
            io::stdout(),
            "{:1} {:1} {:<12} {:>5} {:>5} {:>2} {:>3} {:>3} {:>4} {:>7} {:<6} {:<8} {:<12} {:<8} {}",
            "F", "S", "UID", "PID", "PPID", "C", "PRI", "NI", "ADDR", "SZ", "WCHAN", "STIME", "TTY", "TIME", "CMD"
        ).is_err() {
            return;
        }
    } else if cli.l {
        if writeln!(
            io::stdout(),
            "{:1} {:1} {:<8} {:>5} {:>5} {:>2} {:>3} {:>3} {:>4} {:>7} {:<6} {:<8} {:<8} {:<8} {}",
            "F", "S", "UID", "PID", "PPID", "C", "PRI", "NI", "ADDR", "SZ", "WCHAN", "STIME", "TTY", "TIME", "CMD"
        ).is_err() {
            return;
        }
    } else if cli.f || cli.e {
        if writeln!(
            io::stdout(),
            "{: <12} {: >5} {: >5} {: >2} {: <8} {: <12} {: <8} {}",
            "UID", "PID", "PPID", "C", "STIME", "TTY", "TIME", "CMD"
        ).is_err() {
            return;
        }
    } else {
        if writeln!(io::stdout(), "{: >5} {: <12} {: <8} {}", "PID", "TTY", "TIME", "CMD").is_err() {
            return;
        }
    }

    let current_pid = std::process::id();
    let current_user_id = sys.process(Pid::from(current_pid as usize)).unwrap().user_id().unwrap();

    for (pid, process) in sys.processes() {
        if !cli.e && process.user_id() != Some(current_user_id) {
            continue;
        }

        let user_name = match process.user_id() {
            Some(user_id) => users
                .iter()
                .find(|u| u.id() == user_id)
                .map(|u| u.name().to_string())
                .unwrap_or_else(|| "-".to_string()),
            None => "-".to_string(),
        };

        let start_time = Local.timestamp_opt(process.start_time() as i64, 0).unwrap();
        let total_run_time = process.run_time();
        let hours = total_run_time / 3600;
        let minutes = (total_run_time % 3600) / 60;
        let seconds = total_run_time % 60;

        let cmd = if process.cmd().is_empty() {
            format!("[{}]", process.name())
        } else {
            process.cmd().join(" ")
        };

        let display_cmd = if cli.f || cli.l {
            cmd
        } else {
            process.name().to_string()
        };

        let result = if cli.l && cli.f {
            writeln!(
                io::stdout(),
                "{:1} {:1} {:<12} {:>5} {:>5} {:>2} {:>3} {:>3} {:>4} {:>7} {:<6} {:<8} {:<12} {:02}:{:02}:{:02} {}",
                "4",
                process.status().to_string(),
                user_name,
                pid,
                process.parent().unwrap_or(Pid::from(0)),
                (process.cpu_usage() * 100.0) as u8,
                "80",
                "0",
                "-",
                process.memory() / 1024,
                "-",
                start_time.format("%b%d"),
                "?",
                hours,
                minutes,
                seconds,
                display_cmd
            )
        } else if cli.l {
            writeln!(
                io::stdout(),
                "{:1} {:1} {:<8} {:>5} {:>5} {:>2} {:>3} {:>3} {:>4} {:>7} {:<6} {:<8} {:<8} {:02}:{:02}:{:02} {}",
                "4",
                process.status().to_string(),
                user_name,
                pid,
                process.parent().unwrap_or(Pid::from(0)),
                (process.cpu_usage() * 100.0) as u8,
                "80",
                "0",
                "-",
                process.memory() / 1024,
                "-",
                start_time.format("%b%d"),
                "?",
                hours,
                minutes,
                seconds,
                display_cmd
            )
        } else if cli.f || cli.e {
            writeln!(
                io::stdout(),
                "{: <12} {: >5} {: >5} {: >2} {: <8} {: <12} {:02}:{:02}:{:02} {}",
                user_name,
                pid,
                process.parent().unwrap_or(Pid::from(0)),
                (process.cpu_usage() * 100.0) as u8,
                start_time.format("%b%d"),
                "?",
                hours,
                minutes,
                seconds,
                display_cmd
            )
        } else {
            writeln!(
                io::stdout(),
                "{: >5} {: <12} {:02}:{:02}:{:02} {}",
                pid,
                "?",
                hours,
                minutes,
                seconds,
                display_cmd
            )
        };
        if result.is_err() {
            break;
        }
    }
}
