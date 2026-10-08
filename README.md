# psw - Process Status for Windows

`psw` is a `ps`-like command-line tool that lists running processes. It is written for Windows
(it also builds on Linux/macOS via `sysinfo`). Only columns whose values the OS really provides
are printed; there are no placeholder columns (TTY, PRI, NI, ADDR, WCHAN, F are intentionally absent).

## Build

```
cargo build --release
```

## Usage

```
psw [-e] [-l] [-f]
```

| Option | Long | Meaning | Columns |
|---|---|---|---|
| (none) | | Current user's processes | `PID TIME CMD` (CMD = process name) |
| `-e` | `--all` | All users' processes | `UID PID PPID %CPU STIME TIME CMD` (CMD = process name) |
| `-f` | `--full` | Full format, full command line | `UID PID PPID %CPU STIME TIME CMD` |
| `-l` | `--long` | Long format, full command line | `STAT UID PID PPID %CPU RSS STIME TIME CMD` |

Options can be combined (`-e -l` = all users, long format). Output is sorted by PID and column
widths adapt to the data.

## Column semantics

| Column | Meaning |
|---|---|
| `TIME` | Accumulated **CPU time** (kernel + user), `HH:MM:SS` (not wall-clock uptime) |
| `%CPU` | Instantaneous CPU usage sampled over ~200 ms. Relative to one core, so it can exceed 100 on multi-core machines. `-e/-f/-l` wait ~200 ms for this sample; the default mode does not |
| `RSS` | Resident memory in KiB |
| `STIME` | Start time: `HH:MM` if started today, otherwise `MonDD` |
| `STAT` | Process status as reported by the OS |
| `UID` | User name; falls back to the raw user id if it cannot be resolved, `-` if unavailable |
| `PPID` | Parent PID, `-` if unknown |

## Errors

If the current user cannot be determined (default mode only), `psw` prints an error to stderr and
exits with status 1; use `-e` to list all processes. A closed pipe (`psw | head`) exits quietly.
