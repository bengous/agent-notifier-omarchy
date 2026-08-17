use std::fs;
use std::str::FromStr;

use crate::event::ProcessRef;

/// Field indexes into `/proc/<pid>/stat`, counted after `comm`: that field is a
/// program name in parentheses and may hold spaces, so only the text past its
/// last `)` splits into fields. proc(5) numbers `ppid` 4 and `starttime` 22.
const PARENT_PID_FIELD: usize = 1;
const START_TIME_FIELD: usize = 19;

pub(crate) fn process_ref(pid: i64) -> Option<ProcessRef> {
    Some(ProcessRef {
        pid,
        start_time: proc_stat_field(pid, START_TIME_FIELD)?,
    })
}

pub(crate) fn process_is_alive(process: &ProcessRef) -> bool {
    proc_stat_field(process.pid, START_TIME_FIELD) == Some(process.start_time)
}

pub(in crate::window) fn current_parent_pid() -> i64 {
    let own_pid = i64::from(std::process::id());
    read_parent_pid(own_pid).unwrap_or(own_pid)
}

pub(in crate::window) fn pid_chain(start_pid: i64, max_depth: usize) -> Vec<i64> {
    let mut chain = Vec::new();
    let mut current = Some(start_pid);
    for _ in 0..max_depth {
        let Some(pid) = current else { break };
        if pid <= 0 {
            break;
        }
        chain.push(pid);
        current = read_parent_pid(pid);
    }
    chain
}

fn read_parent_pid(pid: i64) -> Option<i64> {
    proc_stat_field(pid, PARENT_PID_FIELD).filter(|parent| *parent > 0)
}

fn proc_stat_field<T: FromStr>(pid: i64, index: usize) -> Option<T> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close_paren = stat.rfind(')')?;
    stat.get(close_paren + 2..)?
        .split_whitespace()
        .nth(index)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests;
