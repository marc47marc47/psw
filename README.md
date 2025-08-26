# psw - Process Status for Windows

`psw` is a command-line tool for Windows that provides information about the currently running processes, similar to the `ps` command on Linux.

## Features

- List processes with various levels of detail.
- Filter processes by the current user.
- Gracefully handles errors and processes with missing information.

## Usage

### Default

By default, `psw` lists the processes belonging to the current user.

```
psw
```

**Output:**

```
  PID TTY          TIME CMD
2523195 pts/2    00:00:00 bash
3891965 pts/2    00:00:00 psw
```

### Options

- `-e`: Select all processes.
- `-l`: Long format.
- `-f`: Full format.

These options can be combined.

#### `-e` - All Processes

Lists all processes running on the system.

```
psw -e
```

**Output:**

```
UID          PID    PPID  C STIME TTY          TIME CMD
dbsecure 2523195 2523194  0 Aug26 pts/2    00:00:00 -bash
root     3878164 2523195  0 01:26 pts/2    00:00:00 ps -e
```

#### `-f` - Full Format

Provides a more detailed output.

```
psw -f
```

**Output:**

```
UID          PID    PPID  C STIME TTY          TIME CMD
dbsecure 2523195 2523194  0 Aug26 pts/2    00:00:00 -bash
dbsecure 3878164 2523195  0 01:26 pts/2    00:00:00 ps -f
```

#### `-l` - Long Format

Provides a long format output with even more details.

```
psw -l
```

**Output:**

```
F S   UID     PID    PPID  C PRI  NI ADDR SZ WCHAN  TTY          TIME CMD
4 S  1000 2523195 2523194  0  80   0 -  7481 -      pts/2    00:00:00 bash
0 R  1000 3877235 2523195  0  80   0 - 11377 -      pts/2    00:00:00 ps
```

#### `-lf` - Combined Long and Full Format

Combines the long and full format options.

```
psw -lf
```

**Output:**

```
F S UID          PID    PPID  C PRI  NI ADDR SZ WCHAN  STIME TTY          TIME CMD
4 S dbsecure 2523195 2523194  0  80   0 -  7481 -      Aug26 pts/2    00:00:00 -bash
0 R dbsecure 3875887 2523195  0  80   0 - 14691 -      01:25 pts/2    00:00:00 ps -lf
```
