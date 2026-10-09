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

## Tree view: `psw tree`

pstree-like output. Options go **after** `tree` (the list options `-e/-l/-f` are rejected there).

```
psw tree [-a] [-l] [-n] [-p] [-s] [-A] [PID|NAME]
```

| Option | Meaning |
|---|---|
| `-a`, `--arguments` | Show command-line arguments (`name,pid args` when combined with `-p`) |
| `-l`, `--long` | Wrap long lines (continuation lines stay aligned). Default on a terminal: truncate to the terminal width |
| `-n`, `--numeric-sort` | Sort siblings by PID (default: by name, case-insensitive) |
| `-p`, `--show-pids` | Show PIDs: `name(pid)` |
| `-s`, `--show-parents` | Also show the **ancestors** of the selected processes. With no `PID\|NAME` the selected process is `psw` itself, i.e. "where am I in the process tree?" |
| `-A`, `--ascii` | ASCII line drawing (use this if the console cannot show `├─ └─ │`, e.g. legacy code pages) |
| `PID\|NAME` | Show only that process and its descendants. A number is a PID, anything else a name (case-insensitive, `.exe` optional, all matches are shown). Combine with `-s` to add the ancestors |

```
$ psw tree -sp          # where am I?
System(4)
  └─...
      └─pwsh.exe(7340)
          └─psw.exe(9120)

$ psw tree -nlaps       # same flags as pstree
```

Behaviour notes:

* Output is **never** truncated or wrapped when stdout is not a terminal (`psw tree | grep ...` sees full lines).
* A process whose parent has exited, whose parent PID was **reused** by a younger process
  (detected by comparing start times), or that is part of a corrupt cycle is shown as a top-level
  node instead of being attached to a wrong parent.
* The tree always includes all users' processes (like `pstree`); processes you cannot inspect
  may show an empty command line.
* Not implemented vs. `pstree`: merging identical sibling subtrees (`3*[bash]`, i.e. always `-c`),
  threads (`{name}`), `-u/-h/-g` options.

## Errors

If the current user cannot be determined (default mode only), `psw` prints an error to stderr and
exits with status 1; use `-e` to list all processes. A closed pipe (`psw | head`) exits quietly.
