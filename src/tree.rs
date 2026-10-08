//! Process tree (pstree-like): forest construction, `-s` selection and rendering.
//!
//! Everything here is pure (no OS access) so the awkward cases - orphans, PID reuse, cycles -
//! can be unit tested.

use crate::text::{display_width, sanitize, truncate_to_width, wrap_to_width};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub pid: u32,
    pub ppid: Option<u32>,
    /// Start time in seconds since the epoch; used to reject reused parent PIDs.
    pub start: u64,
    pub name: String,
    /// Command line without argv[0].
    pub args: String,
}

#[derive(Debug)]
pub struct Forest {
    pub parent: Vec<Option<usize>>,
    pub children: Vec<Vec<usize>>,
    numeric_sort: bool,
}

fn sort_siblings(nodes: &[TreeNode], numeric_sort: bool, v: &mut [usize]) {
    if numeric_sort {
        v.sort_by_key(|&i| nodes[i].pid);
    } else {
        v.sort_by_cached_key(|&i| (nodes[i].name.to_lowercase(), nodes[i].pid));
    }
}

/// Links every node to its parent. A node becomes a root when its parent is unusable:
///
/// * missing (the parent already exited - very common on Windows),
/// * itself (PID 0),
/// * younger than the child (the parent PID was reused by an unrelated process),
/// * part of a cycle (corrupt / racy data) - the cycle is cut at one node.
pub fn build_forest(nodes: &[TreeNode], numeric_sort: bool) -> Forest {
    let by_pid: HashMap<u32, usize> = nodes.iter().enumerate().map(|(i, n)| (n.pid, i)).collect();

    let mut parent: Vec<Option<usize>> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let p = *by_pid.get(&n.ppid?)?;
            (p != i && nodes[p].start <= n.start).then_some(p)
        })
        .collect();

    for i in 0..nodes.len() {
        let mut cur = i;
        let mut steps = 0;
        while let Some(p) = parent[cur] {
            cur = p;
            steps += 1;
            if steps > nodes.len() {
                parent[i] = None;
                break;
            }
        }
    }

    let mut children = vec![Vec::new(); nodes.len()];
    for (i, p) in parent.iter().enumerate() {
        if let Some(p) = p {
            children[*p].push(i);
        }
    }

    children
        .iter_mut()
        .for_each(|c| sort_siblings(nodes, numeric_sort, c));

    Forest {
        parent,
        children,
        numeric_sort,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Selector {
    Pid(u32),
    Name(String),
    /// No argument given: the running `psw` process itself ("where am I in the tree?").
    SelfProcess,
}

impl Selector {
    pub fn parse(arg: Option<&str>) -> Selector {
        match arg {
            None => Selector::SelfProcess,
            Some(a) => match a.parse::<u32>() {
                Ok(pid) => Selector::Pid(pid),
                Err(_) => Selector::Name(a.to_string()),
            },
        }
    }
}

fn normalize_name(s: &str) -> String {
    let lower = s.to_lowercase();
    lower
        .strip_suffix(".exe")
        .map(str::to_string)
        .unwrap_or(lower)
}

/// Resolves the `-s` selector to node indexes.
pub fn find_targets(
    nodes: &[TreeNode],
    sel: &Selector,
    self_pid: Option<u32>,
) -> Result<Vec<usize>, String> {
    let found: Vec<usize> = match sel {
        Selector::Pid(pid) => nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.pid == *pid)
            .map(|(i, _)| i)
            .collect(),
        Selector::SelfProcess => {
            let pid = self_pid.ok_or("cannot determine the current process id")?;
            nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.pid == pid)
                .map(|(i, _)| i)
                .collect()
        }
        Selector::Name(name) => {
            let want = normalize_name(name);
            nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| normalize_name(&n.name) == want)
                .map(|(i, _)| i)
                .collect()
        }
    };
    if found.is_empty() {
        Err(match sel {
            Selector::Pid(pid) => format!("no process with PID {pid}"),
            Selector::Name(name) => format!("no process named '{name}'"),
            Selector::SelfProcess => {
                "the current process was not found in the process list".to_string()
            }
        })
    } else {
        Ok(found)
    }
}

/// Nodes to display for a selection: every target and all its descendants, plus - with
/// `with_parents` (`-s`) - all its ancestors.
pub fn select_subset(forest: &Forest, targets: &[usize], with_parents: bool) -> Vec<bool> {
    let mut keep = vec![false; forest.parent.len()];
    for &t in targets {
        if with_parents {
            let mut cur = Some(t);
            while let Some(i) = cur {
                keep[i] = true;
                cur = forest.parent[i];
            }
        }
        let mut stack = vec![t];
        while let Some(i) = stack.pop() {
            keep[i] = true;
            stack.extend(forest.children[i].iter().copied());
        }
    }
    keep
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub show_pids: bool,
    pub show_args: bool,
    pub ascii: bool,
}

/// How lines are fitted to the terminal. `width: None` (not a terminal) never cuts anything.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub width: Option<usize>,
    pub wrap: bool,
}

struct Glyphs {
    branch: &'static str,
    last: &'static str,
    bar_cont: &'static str,
    blank_cont: &'static str,
    bar_child: &'static str,
    blank_child: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    branch: "├─",
    last: "└─",
    bar_cont: "│ ",
    blank_cont: "  ",
    bar_child: "│   ",
    blank_child: "    ",
};

const ASCII: Glyphs = Glyphs {
    branch: "|-",
    last: "`-",
    bar_cont: "| ",
    blank_cont: "  ",
    bar_child: "|   ",
    blank_child: "    ",
};

/// `name`, `name(pid)`, or - with args - `name,pid args` (same as pstree).
pub fn node_label(n: &TreeNode, style: Style) -> String {
    let mut s = n.name.clone();
    if style.show_pids {
        if style.show_args {
            s.push_str(&format!(",{}", n.pid));
        } else {
            s.push_str(&format!("({})", n.pid));
        }
    }
    if style.show_args && !n.args.is_empty() {
        s.push(' ');
        s.push_str(&n.args);
    }
    sanitize(&s)
}

fn layout_label(prefix: &str, cont: &str, label: &str, layout: Layout) -> Vec<String> {
    // One cell is left free: writing exactly the last column makes legacy Windows consoles wrap early.
    match layout.width.map(|w| w.saturating_sub(1).max(1)) {
        None => vec![format!("{prefix}{label}")],
        Some(w) if !layout.wrap => vec![truncate_to_width(&format!("{prefix}{label}"), w)],
        Some(w) => {
            let first = w.saturating_sub(display_width(prefix));
            let rest = w.saturating_sub(display_width(cont));
            wrap_to_width(label, first, rest)
                .into_iter()
                .enumerate()
                .map(|(i, part)| {
                    if i == 0 {
                        format!("{prefix}{part}")
                    } else {
                        format!("{cont}{part}")
                    }
                })
                .collect()
        }
    }
}

/// Renders the forest (optionally restricted to `keep`) as text lines, depth first.
pub fn render_tree(
    nodes: &[TreeNode],
    forest: &Forest,
    keep: Option<&[bool]>,
    style: Style,
    layout: Layout,
) -> Vec<String> {
    let g = if style.ascii { &ASCII } else { &UNICODE };
    let visible = |i: usize| keep.is_none_or(|k| k[i]);

    // (node, line prefix, continuation prefix for wrapped text, prefix inherited by children)
    // With a selection, a kept node whose parent is hidden (plain `psw tree PID`) is a top node too.
    let mut tops: Vec<usize> = (0..nodes.len())
        .filter(|&i| visible(i) && forest.parent[i].is_none_or(|p| !visible(p)))
        .collect();
    sort_siblings(nodes, forest.numeric_sort, &mut tops);
    let mut stack: Vec<(usize, String, String, String)> = tops
        .into_iter()
        .rev()
        .map(|r| (r, String::new(), String::new(), "  ".to_string()))
        .collect();

    let mut out = Vec::with_capacity(nodes.len());
    while let Some((i, prefix, cont, child_cp)) = stack.pop() {
        out.extend(layout_label(
            &prefix,
            &cont,
            &node_label(&nodes[i], style),
            layout,
        ));

        let kids: Vec<usize> = forest.children[i]
            .iter()
            .copied()
            .filter(|&c| visible(c))
            .collect();
        for (pos, &c) in kids.iter().enumerate().rev() {
            let is_last = pos + 1 == kids.len();
            stack.push((
                c,
                format!("{child_cp}{}", if is_last { g.last } else { g.branch }),
                format!(
                    "{child_cp}{}",
                    if is_last { g.blank_cont } else { g.bar_cont }
                ),
                format!(
                    "{child_cp}{}",
                    if is_last { g.blank_child } else { g.bar_child }
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(pid: u32, ppid: Option<u32>, start: u64, name: &str) -> TreeNode {
        TreeNode {
            pid,
            ppid,
            start,
            name: name.to_string(),
            args: String::new(),
        }
    }

    const PLAIN: Style = Style {
        show_pids: false,
        show_args: false,
        ascii: false,
    };
    const NOCUT: Layout = Layout {
        width: None,
        wrap: false,
    };

    fn roots(f: &Forest) -> Vec<usize> {
        (0..f.parent.len())
            .filter(|&i| f.parent[i].is_none())
            .collect()
    }

    fn sample() -> Vec<TreeNode> {
        vec![
            n(1, None, 10, "init"),
            n(20, Some(1), 11, "sshd"),
            n(30, Some(1), 11, "Auditd"),
            n(21, Some(20), 12, "bash"),
            n(22, Some(20), 12, "bash"),
        ]
    }

    #[test]
    fn renders_pstree_style_sorted_by_name() {
        let nodes = sample();
        let f = build_forest(&nodes, false);
        assert_eq!(
            render_tree(&nodes, &f, None, PLAIN, NOCUT),
            vec![
                "init",
                "  ├─Auditd",
                "  └─sshd",
                "      ├─bash",
                "      └─bash",
            ]
        );
    }

    #[test]
    fn numeric_sort_pids_and_args() {
        let mut nodes = sample();
        nodes[3].args = "-l".to_string();
        let f = build_forest(&nodes, true);
        let style = Style {
            show_pids: true,
            show_args: true,
            ascii: false,
        };
        assert_eq!(
            render_tree(&nodes, &f, None, style, NOCUT),
            vec![
                "init,1",
                "  ├─sshd,20",
                "  │   ├─bash,21 -l",
                "  │   └─bash,22",
                "  └─Auditd,30",
            ]
        );
        let style = Style {
            show_pids: true,
            show_args: false,
            ascii: true,
        };
        let lines = render_tree(&nodes, &f, None, style, NOCUT);
        assert_eq!(lines[1], "  |-sshd(20)");
        assert_eq!(lines[2], "  |   |-bash(21)");
        assert_eq!(lines[4], "  `-Auditd(30)");
    }

    #[test]
    fn orphan_becomes_root() {
        let nodes = vec![n(5, Some(999), 10, "orphan"), n(6, Some(5), 11, "kid")];
        let f = build_forest(&nodes, false);
        assert_eq!(roots(&f), vec![0]);
        assert_eq!(
            render_tree(&nodes, &f, None, PLAIN, NOCUT),
            vec!["orphan", "  └─kid"]
        );
    }

    #[test]
    fn self_parent_is_root() {
        let nodes = vec![n(0, Some(0), 0, "idle")];
        let f = build_forest(&nodes, false);
        assert_eq!(roots(&f), vec![0]);
    }

    #[test]
    fn reused_parent_pid_is_rejected() {
        // pid 10 was started at t=100, but the "child" claims it as parent and started at t=50:
        // the real parent died and its PID was reused by an unrelated, younger process.
        let nodes = vec![
            n(10, None, 100, "new-unrelated"),
            n(11, Some(10), 50, "old-child"),
        ];
        let f = build_forest(&nodes, false);
        assert_eq!(roots(&f).len(), 2);
        assert!(f.parent[1].is_none());
    }

    #[test]
    fn cycle_is_cut_and_everything_still_renders() {
        let nodes = vec![
            n(1, Some(2), 5, "a"),
            n(2, Some(1), 5, "b"),
            n(3, Some(2), 6, "c"),
        ];
        let f = build_forest(&nodes, false);
        assert!(!roots(&f).is_empty());
        let lines = render_tree(&nodes, &f, None, PLAIN, NOCUT);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn selector_parsing() {
        assert_eq!(Selector::parse(None), Selector::SelfProcess);
        assert_eq!(Selector::parse(Some("1234")), Selector::Pid(1234));
        assert_eq!(
            Selector::parse(Some("chrome")),
            Selector::Name("chrome".into())
        );
    }

    #[test]
    fn find_targets_cases() {
        let nodes = vec![
            n(1, None, 1, "System"),
            n(2, Some(1), 2, "Chrome.exe"),
            n(3, Some(1), 2, "chrome"),
        ];
        assert_eq!(find_targets(&nodes, &Selector::Pid(2), None), Ok(vec![1]));
        assert_eq!(
            find_targets(&nodes, &Selector::Name("CHROME".into()), None),
            Ok(vec![1, 2])
        );
        assert_eq!(
            find_targets(&nodes, &Selector::Name("chrome.exe".into()), None),
            Ok(vec![1, 2])
        );
        assert_eq!(
            find_targets(&nodes, &Selector::SelfProcess, Some(3)),
            Ok(vec![2])
        );
        assert!(find_targets(&nodes, &Selector::Pid(99), None).is_err());
        assert!(find_targets(&nodes, &Selector::Name("nope".into()), None).is_err());
        assert!(find_targets(&nodes, &Selector::SelfProcess, None).is_err());
        assert!(find_targets(&nodes, &Selector::SelfProcess, Some(77)).is_err());
    }

    #[test]
    fn show_parents_keeps_ancestors_and_descendants_only() {
        let nodes = sample();
        let f = build_forest(&nodes, false);
        let keep = select_subset(&f, &[1], true); // sshd
        assert_eq!(
            render_tree(&nodes, &f, Some(&keep), PLAIN, NOCUT),
            vec!["init", "  └─sshd", "      ├─bash", "      └─bash"]
        );
        let keep = select_subset(&f, &[3], true); // one bash: ancestors, no siblings
        assert_eq!(
            render_tree(&nodes, &f, Some(&keep), PLAIN, NOCUT),
            vec!["init", "  └─sshd", "      └─bash"]
        );
    }

    #[test]
    fn without_parents_only_the_subtree_is_kept() {
        let nodes = sample();
        let f = build_forest(&nodes, false);
        let keep = select_subset(&f, &[1], false); // sshd, no -s
        assert_eq!(
            render_tree(&nodes, &f, Some(&keep), PLAIN, NOCUT),
            vec!["sshd", "  ├─bash", "  └─bash"]
        );
    }

    #[test]
    fn parents_chain_stops_at_reused_pid() {
        let nodes = vec![
            n(1, None, 1, "grand"),
            n(2, Some(1), 100, "reused"), // started after its child -> link to child is invalid
            n(3, Some(2), 50, "child"),
        ];
        let f = build_forest(&nodes, false);
        let keep = select_subset(&f, &[2], true);
        assert_eq!(keep, vec![false, false, true]);
    }

    #[test]
    fn truncate_and_wrap_layout() {
        let mut nodes = sample();
        nodes[1].name = "sshd-with-a-very-long-name".to_string();
        let f = build_forest(&nodes, false);
        let keep = select_subset(&f, &[1], true);

        // width 16 -> usable 15 cells
        let cut = render_tree(
            &nodes,
            &f,
            Some(&keep),
            PLAIN,
            Layout {
                width: Some(16),
                wrap: false,
            },
        );
        assert_eq!(cut[1], "  └─sshd-with-a");
        assert!(cut.iter().all(|l| display_width(l) <= 15));

        let wrapped = render_tree(
            &nodes,
            &f,
            Some(&keep),
            PLAIN,
            Layout {
                width: Some(16),
                wrap: true,
            },
        );
        assert_eq!(wrapped[1], "  └─sshd-with-a");
        assert_eq!(wrapped[2], "    -very-long-");
        assert_eq!(wrapped[3], "    name");
        assert!(wrapped.iter().all(|l| display_width(l) <= 15));
        assert_eq!(wrapped.iter().filter(|l| l.contains("bash")).count(), 2);
    }

    #[test]
    fn control_chars_are_sanitized() {
        let mut x = n(1, None, 1, "a\nb");
        x.args = "x\ty".to_string();
        let style = Style {
            show_pids: false,
            show_args: true,
            ascii: false,
        };
        assert_eq!(node_label(&x, style), "a b x y");
    }
}
