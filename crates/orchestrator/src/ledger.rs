//! The task ledger: the orchestrator's structured shared state (the
//! "blackboard"), persisted as JSON. Holds the task DAG and statuses so no
//! single agent context has to carry the whole plan.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where a task is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    /// Ready by deps, but its region is leased by another actor.
    Blocked,
}

/// One unit of work.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub intent: String,
    /// Task ids that must complete before this one.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Target path globs the task is expected to touch (drives footprint/lease).
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default = "pending")]
    pub status: TaskStatus,
}

fn pending() -> TaskStatus {
    TaskStatus::Pending
}

impl Task {
    pub fn new(id: &str, intent: &str) -> Task {
        Task {
            id: id.to_string(),
            intent: intent.to_string(),
            deps: Vec::new(),
            targets: Vec::new(),
            status: TaskStatus::Pending,
        }
    }

    pub fn with_deps(mut self, deps: &[&str]) -> Task {
        self.deps = deps.iter().map(|d| d.to_string()).collect();
        self
    }

    pub fn with_targets(mut self, targets: &[&str]) -> Task {
        self.targets = targets.iter().map(|t| t.to_string()).collect();
        self
    }
}

/// The DAG of tasks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub tasks: Vec<Task>,
}

impl Ledger {
    pub fn new() -> Ledger {
        Ledger::default()
    }

    pub fn load(path: &Path) -> io::Result<Ledger> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Ledger::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }

    pub fn add(&mut self, task: Task) {
        self.tasks.push(task);
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn set_status(&mut self, id: &str, status: TaskStatus) {
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) {
            task.status = status;
        }
    }

    /// True if the dependency graph contains a cycle (or a dangling dep).
    pub fn has_cycle(&self) -> bool {
        let ids: HashSet<&str> = self.tasks.iter().map(|task| task.id.as_str()).collect();
        // A dep on a non-existent task is malformed; treat as unschedulable.
        if self
            .tasks
            .iter()
            .any(|task| task.deps.iter().any(|dep| !ids.contains(dep.as_str())))
        {
            return true;
        }
        let edges: HashMap<&str, &[String]> = self
            .tasks
            .iter()
            .map(|task| (task.id.as_str(), task.deps.as_slice()))
            .collect();
        let mut visiting: HashSet<&str> = HashSet::new();
        let mut done: HashSet<&str> = HashSet::new();
        self.tasks
            .iter()
            .any(|task| visit_cycle(task.id.as_str(), &edges, &mut visiting, &mut done))
    }

    /// Tasks that are `Pending` and whose deps are all `Done` — eligible to run.
    pub fn ready(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Pending && self.deps_done(task))
            .collect()
    }

    fn deps_done(&self, task: &Task) -> bool {
        task.deps
            .iter()
            .all(|dep| self.get(dep).map(|d| d.status) == Some(TaskStatus::Done))
    }
}

/// DFS cycle check (white/grey/black).
fn visit_cycle<'a>(
    id: &'a str,
    edges: &HashMap<&'a str, &'a [String]>,
    visiting: &mut HashSet<&'a str>,
    done: &mut HashSet<&'a str>,
) -> bool {
    if done.contains(id) {
        return false;
    }
    if !visiting.insert(id) {
        return true; // back-edge → cycle
    }
    if let Some(deps) = edges.get(id) {
        for dep in deps.iter() {
            if visit_cycle(dep.as_str(), edges, visiting, done) {
                return true;
            }
        }
    }
    visiting.remove(id);
    done.insert(id);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_respects_deps() {
        let mut ledger = Ledger::new();
        ledger.add(Task::new("a", "first"));
        ledger.add(Task::new("b", "second").with_deps(&["a"]));
        // only `a` is ready initially.
        assert_eq!(
            ledger.ready().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["a"]
        );
        ledger.set_status("a", TaskStatus::Done);
        assert_eq!(
            ledger.ready().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["b"]
        );
    }

    #[test]
    fn detects_cycles_and_dangling_deps() {
        let mut cyclic = Ledger::new();
        cyclic.add(Task::new("a", "x").with_deps(&["b"]));
        cyclic.add(Task::new("b", "y").with_deps(&["a"]));
        assert!(cyclic.has_cycle());

        let mut acyclic = Ledger::new();
        acyclic.add(Task::new("a", "x"));
        acyclic.add(Task::new("b", "y").with_deps(&["a"]));
        assert!(!acyclic.has_cycle());

        let mut dangling = Ledger::new();
        dangling.add(Task::new("a", "x").with_deps(&["ghost"]));
        assert!(dangling.has_cycle());
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("ad-ledger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ledger.json");
        let mut ledger = Ledger::new();
        ledger.add(Task::new("a", "do a").with_targets(&["src/a/**"]));
        ledger.save(&path).unwrap();
        let loaded = Ledger::load(&path).unwrap();
        assert_eq!(loaded.tasks, ledger.tasks);
        std::fs::remove_dir_all(&dir).ok();
    }
}
