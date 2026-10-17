use std::collections::HashMap;

use crate::config::{ProcCfg, SortKey};
use crate::model::ProcInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub idx: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sig {
    Term,
    Kill,
}

fn cmp(a: &ProcInfo, b: &ProcInfo, sort: SortKey) -> std::cmp::Ordering {
    match sort {
        SortKey::Cpu => b.cpu.total_cmp(&a.cpu).then(a.pid.cmp(&b.pid)),
        SortKey::Mem => b.mem.cmp(&a.mem).then(a.pid.cmp(&b.pid)),
        SortKey::Pid => a.pid.cmp(&b.pid),
        SortKey::Name => a
            .name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.pid.cmp(&b.pid)),
    }
}

pub fn arrange(
    procs: &[ProcInfo],
    filter: &str,
    sort: SortKey,
    reverse: bool,
    tree: bool,
) -> Vec<Row> {
    let f = filter.to_lowercase();
    let mut idx: Vec<usize> = (0..procs.len())
        .filter(|i| f.is_empty() || procs[*i].name.to_lowercase().contains(&f))
        .collect();
    let order = |a: &usize, b: &usize| {
        let o = cmp(&procs[*a], &procs[*b], sort);
        if reverse { o.reverse() } else { o }
    };
    idx.sort_by(order);
    if !tree || !f.is_empty() {
        return idx.into_iter().map(|i| Row { idx: i, depth: 0 }).collect();
    }
    let by_pid: HashMap<u32, usize> = idx.iter().map(|i| (procs[*i].pid, *i)).collect();
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for i in &idx {
        match procs[*i]
            .ppid
            .filter(|pp| by_pid.contains_key(pp) && *pp != procs[*i].pid)
        {
            Some(pp) => children.entry(pp).or_default().push(*i),
            None => roots.push(*i),
        }
    }
    let mut out = Vec::with_capacity(idx.len());
    let mut stack: Vec<(usize, usize)> = roots.into_iter().rev().map(|i| (i, 0)).collect();
    let mut seen = std::collections::HashSet::new();
    while let Some((i, d)) = stack.pop() {
        if !seen.insert(i) {
            continue; // cycle guard
        }
        out.push(Row { idx: i, depth: d });
        if let Some(ch) = children.get(&procs[i].pid) {
            for c in ch.iter().rev() {
                stack.push((*c, d + 1));
            }
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct ProcView {
    pub filter: String,
    pub sort: SortKey,
    pub reverse: bool,
    pub tree: bool,
    pub selected_pid: Option<u32>,
    pub cursor: usize,
    pub offset: usize,
    /// Rows visible in the last render; PgUp/PgDn move by this much.
    pub page: usize,
}

impl ProcView {
    pub fn from_cfg(c: &ProcCfg) -> Self {
        Self {
            sort: c.sort,
            reverse: c.reverse,
            tree: c.tree,
            ..Default::default()
        }
    }

    /// Re-anchors the cursor on `selected_pid` after the rows changed; clamps otherwise.
    pub fn sync(&mut self, rows: &[Row], procs: &[ProcInfo]) {
        if let Some(pid) = self.selected_pid
            && let Some(pos) = rows.iter().position(|r| procs[r.idx].pid == pid)
        {
            self.cursor = pos;
            return;
        }
        self.cursor = self.cursor.min(rows.len().saturating_sub(1));
        self.selected_pid = rows.get(self.cursor).map(|r| procs[r.idx].pid);
    }

    pub fn move_by(&mut self, delta: isize, rows: &[Row], procs: &[ProcInfo]) {
        if rows.is_empty() {
            self.cursor = 0;
            self.selected_pid = None;
            return;
        }
        let max = rows.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, max) as usize;
        self.selected_pid = Some(procs[rows[self.cursor].idx].pid);
    }

    pub fn top(&mut self, rows: &[Row], procs: &[ProcInfo]) {
        self.move_by(-(rows.len() as isize), rows, procs);
    }

    pub fn bottom(&mut self, rows: &[Row], procs: &[ProcInfo]) {
        self.move_by(rows.len() as isize, rows, procs);
    }

    pub fn selected<'a>(&self, rows: &[Row], procs: &'a [ProcInfo]) -> Option<&'a ProcInfo> {
        rows.get(self.cursor).map(|r| &procs[r.idx])
    }
}

pub fn errno_message(errno: i32, pid: u32) -> String {
    match errno {
        libc::ESRCH => format!("no such process: {pid}"),
        libc::EPERM => format!("permission denied: {pid}"),
        e => format!("kill {pid} failed: errno {e}"),
    }
}

pub fn send_signal(pid: u32, sig: Sig) -> Result<(), String> {
    let s = match sig {
        Sig::Term => libc::SIGTERM,
        Sig::Kill => libc::SIGKILL,
    };
    let Ok(p) = i32::try_from(pid) else {
        return Err(errno_message(libc::ESRCH, pid));
    };
    if p <= 0 {
        return Err(errno_message(libc::ESRCH, pid));
    }
    // SAFETY: kill(2) with a positive pid only sends a signal; it has no memory-safety preconditions.
    if unsafe { libc::kill(p, s) } == 0 {
        Ok(())
    } else {
        Err(errno_message(
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            pid,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: Option<u32>, name: &str, cpu: f32, mem: u64) -> ProcInfo {
        ProcInfo {
            pid,
            ppid,
            name: name.into(),
            cpu,
            mem,
            ..Default::default()
        }
    }

    fn sample() -> Vec<ProcInfo> {
        vec![
            p(1, None, "launchd", 0.1, 10),
            p(10, Some(1), "Claude", 3.3, 442),
            p(11, Some(10), "Claude Helper", 9.0, 100),
            p(20, Some(1), "screencaptureui", 12.7, 44),
            p(30, Some(1), "zsh", 0.0, 5),
        ]
    }

    fn pids(rows: &[Row], ps: &[ProcInfo]) -> Vec<u32> {
        rows.iter().map(|r| ps[r.idx].pid).collect()
    }

    #[test]
    fn sort_by_each_key() {
        let ps = sample();
        assert_eq!(
            pids(&arrange(&ps, "", SortKey::Cpu, false, false), &ps),
            vec![20, 11, 10, 1, 30]
        );
        assert_eq!(
            pids(&arrange(&ps, "", SortKey::Mem, false, false), &ps),
            vec![10, 11, 20, 1, 30]
        );
        assert_eq!(
            pids(&arrange(&ps, "", SortKey::Pid, false, false), &ps),
            vec![1, 10, 11, 20, 30]
        );
        assert_eq!(
            pids(&arrange(&ps, "", SortKey::Name, false, false), &ps),
            vec![10, 11, 1, 20, 30]
        );
        assert_eq!(
            pids(&arrange(&ps, "", SortKey::Pid, true, false), &ps),
            vec![30, 20, 11, 10, 1]
        );
    }

    #[test]
    fn filter_case_insensitive_and_empty() {
        let ps = sample();
        assert_eq!(
            pids(&arrange(&ps, "claude", SortKey::Pid, false, false), &ps),
            vec![10, 11]
        );
        assert!(arrange(&ps, "nomatch", SortKey::Pid, false, false).is_empty());
    }

    #[test]
    fn tree_orders_children_under_parents() {
        let ps = sample();
        let rows = arrange(&ps, "", SortKey::Cpu, false, true);
        let v: Vec<(u32, usize)> = rows.iter().map(|r| (ps[r.idx].pid, r.depth)).collect();
        assert_eq!(v, vec![(1, 0), (20, 1), (10, 1), (11, 2), (30, 1)]);
    }

    #[test]
    fn tree_with_filter_keeps_matches_flat() {
        let ps = sample();
        let rows = arrange(&ps, "helper", SortKey::Cpu, false, true);
        assert_eq!(rows, vec![Row { idx: 2, depth: 0 }]);
    }

    #[test]
    fn selection_follows_pid_and_clamps() {
        let mut ps = sample();
        let mut v = ProcView::from_cfg(&ProcCfg {
            sort: SortKey::Pid,
            ..Default::default()
        });
        let rows = arrange(&ps, &v.filter, v.sort, v.reverse, v.tree);
        v.sync(&rows, &ps);
        v.move_by(3, &rows, &ps); // pid 20
        assert_eq!(v.selected(&rows, &ps).unwrap().pid, 20);
        // pid 20 moves position after a refresh
        ps.insert(0, p(5, Some(1), "new", 0.0, 1));
        let rows = arrange(&ps, &v.filter, v.sort, v.reverse, v.tree);
        v.sync(&rows, &ps);
        assert_eq!(v.selected(&rows, &ps).unwrap().pid, 20);
        // pid 20 exits → cursor clamps to a valid row
        ps.retain(|x| x.pid != 20 && x.pid != 30);
        let rows = arrange(&ps, &v.filter, v.sort, v.reverse, v.tree);
        v.sync(&rows, &ps);
        assert!(v.cursor < rows.len());
        assert!(v.selected(&rows, &ps).is_some());
    }

    #[test]
    fn empty_list_selection_is_none() {
        let ps: Vec<ProcInfo> = vec![];
        let mut v = ProcView::from_cfg(&ProcCfg::default());
        let rows = arrange(&ps, "", SortKey::Cpu, false, false);
        v.sync(&rows, &ps);
        v.move_by(5, &rows, &ps);
        assert!(v.selected(&rows, &ps).is_none());
        assert_eq!(v.cursor, 0);
    }

    #[test]
    fn esrch_message() {
        assert_eq!(errno_message(libc::ESRCH, 42), "no such process: 42");
        assert_eq!(errno_message(libc::EPERM, 42), "permission denied: 42");
    }

    #[test]
    fn signal_to_dead_pid_fails_cleanly() {
        // pid 0x7ffffffe is practically never in use
        let e = send_signal(0x7fff_fffe, Sig::Term).unwrap_err();
        assert!(e.contains("no such process"), "{e}");
    }
}
