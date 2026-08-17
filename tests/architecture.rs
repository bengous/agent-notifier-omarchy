//! Guards the import edges declared in `.claude/rules/architecture.md`.
//!
//! Only production code is read: a test file may cross concept boundaries to
//! reach a fixture, production code may not.

use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// The concept folders of `src/`. A top-level file is its own concept.
const CONCEPTS: &[&str] = &[
    "alert", "app", "display", "event", "exec", "intake", "window",
];

/// Top-level files that are not concept folders. `main` is the crate root, so
/// any `crate::<item>` that is not a concept resolves to it.
const ROOT_FILES: &[&str] = &["exec", "main", "test_fixtures"];

/// Non-crate targets the rules name explicitly. Any other external crate or
/// `std` path is none of this guard's business.
const WATCHED_EXTERNAL: &[&str] = &["std::fs", "std::os::unix::net"];

/// The concept forbidden to reach each watched external target. Every other
/// concept may: the store owns files, the Hyprland frontier owns the socket.
const FORBIDDEN_EXTERNAL: &[(&str, &str)] = &[("app", "std::fs"), ("app", "std::os::unix::net")];

/// Every edge `.claude/rules/architecture.md` allows. A concept always reaches
/// itself; everything else here fails.
const ALLOWED: &[(&str, &str)] = &[
    ("main", "app"),
    ("app", "display"),
    ("app", "event"),
    ("app", "intake"),
    ("app", "window"),
    ("app", "alert"),
    ("intake", "event"),
    ("intake", "exec"),
    ("window", "event"),
    ("window", "exec"),
    ("display", "event"),
    ("alert", "exec"),
];

/// Edges that exist today and the deepening that removes each one. This list
/// only ever shrinks: a stale entry fails `every_recorded_debt_is_still_real`.
const PLANNED_DEBT: &[Debt] = &[
    Debt {
        from: "app",
        to: "exec",
        // TODO(C4): notify() and play_sound() become the alert concept, the
        // only caller of the subprocess helpers left in this direction.
        step: "C4",
    },
    Debt {
        from: "app",
        to: "main",
        // TODO(C4): the unavailable-status constants are display strings and
        // belong to display, not to the crate root.
        step: "C4",
    },
    Debt {
        from: "app",
        to: "std::fs",
        // TODO(C3): the read that marks the focused window read moves behind
        // the event store, which owns directory creation.
        step: "C3",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Debt {
    from: &'static str,
    to: &'static str,
    step: &'static str,
}

#[test]
fn every_source_file_belongs_to_a_documented_concept() -> Result<(), Box<dyn Error>> {
    let stray = source_files()?
        .into_iter()
        .filter(|path| {
            let concept = concept_of(path);
            !CONCEPTS.contains(&concept.as_str()) && !ROOT_FILES.contains(&concept.as_str())
        })
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();

    assert!(
        stray.is_empty(),
        "these files sit outside the documented tree: {}",
        stray.join(", ")
    );
    Ok(())
}

#[test]
fn imports_follow_the_documented_edges() -> Result<(), Box<dyn Error>> {
    let unexpected = observed_edges()?
        .into_iter()
        .filter(|(from, to)| is_violation(from, to) && recorded_debt(from, to).is_none())
        .map(|(from, to)| format!("{from} -> {to}"))
        .collect::<Vec<_>>();

    assert!(
        unexpected.is_empty(),
        "these imports break .claude/rules/architecture.md: {}. \
         Move the code, or record the edge in PLANNED_DEBT with the deepening that removes it.",
        unexpected.join(", ")
    );
    Ok(())
}

#[test]
fn every_recorded_debt_is_still_real() -> Result<(), Box<dyn Error>> {
    let edges = observed_edges()?;
    let stale = PLANNED_DEBT
        .iter()
        .filter(|debt| !edges.contains(&(debt.from.to_owned(), debt.to.to_owned())))
        .map(|debt| format!("{} -> {} ({})", debt.from, debt.to, debt.step))
        .collect::<Vec<_>>();

    assert!(
        stale.is_empty(),
        "these edges are gone: drop them from PLANNED_DEBT: {}",
        stale.join(", ")
    );
    Ok(())
}

#[test]
fn no_recorded_debt_contradicts_an_allowed_edge() {
    let contradictions = PLANNED_DEBT
        .iter()
        .filter(|debt| !is_violation(debt.from, debt.to))
        .map(|debt| format!("{} -> {}", debt.from, debt.to))
        .collect::<Vec<_>>();

    assert!(
        contradictions.is_empty(),
        "these edges are allowed, so they are not debt: {}",
        contradictions.join(", ")
    );
}

/// `cargo modules dependencies --acyclic` cannot answer this for us: it reads
/// an inherent constructor returning `Self` as a cycle with its own type, and
/// no filter flag removes that pair from the check (cargo-modules 0.27).
#[test]
fn the_allowed_edges_never_form_a_cycle() {
    let looping = ALLOWED
        .iter()
        .filter(|(from, _)| reaches(from, from))
        .map(|(from, to)| format!("{from} -> {to}"))
        .collect::<Vec<_>>();

    assert!(
        looping.is_empty(),
        "these allowed edges close a cycle: {}",
        looping.join(", ")
    );
}

fn reaches(from: &str, target: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut pending = successors(from);
    while let Some(node) = pending.pop() {
        if node == target {
            return true;
        }
        if seen.insert(node) {
            pending.extend(successors(node));
        }
    }
    false
}

fn successors(node: &str) -> Vec<&'static str> {
    ALLOWED
        .iter()
        .filter(|(source, _)| *source == node)
        .map(|(_, destination)| *destination)
        .collect()
}

fn is_violation(from: &str, to: &str) -> bool {
    if WATCHED_EXTERNAL.contains(&to) {
        return FORBIDDEN_EXTERNAL.contains(&(from, to));
    }
    from != to && !ALLOWED.contains(&(from, to))
}

fn recorded_debt(from: &str, to: &str) -> Option<&'static Debt> {
    PLANNED_DEBT
        .iter()
        .find(|debt| debt.from == from && debt.to == to)
}

fn observed_edges() -> Result<BTreeSet<(String, String)>, Box<dyn Error>> {
    let mut edges = BTreeSet::new();
    for path in source_files()? {
        if is_test_file(&path) {
            continue;
        }
        let from = concept_of(&path);
        let mut imports = Imports::default();
        imports.visit_file(&syn::parse_file(&fs::read_to_string(&path)?)?);
        for to in imports.targets {
            edges.insert((from.clone(), to));
        }
    }
    Ok(edges)
}

fn source_files() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension() == Some(OsStr::new("rs")) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_test_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some("tests.rs" | "test_fixtures.rs")
    )
}

/// The folder under `src/` owning the file, or its stem for a top-level file.
fn concept_of(path: &Path) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let relative = path.strip_prefix(&root).unwrap_or(path);
    let mut components = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned());
    let first = components.next().unwrap_or_default();
    if components.next().is_some() {
        return first;
    }
    first.trim_end_matches(".rs").to_owned()
}

#[derive(Debug, Default)]
struct Imports {
    targets: BTreeSet<String>,
}

impl Imports {
    fn record(&mut self, segments: &[String]) {
        match segments.first().map(String::as_str) {
            Some("crate") => {
                let Some(target) = segments.get(1).map(String::as_str) else {
                    return;
                };
                let target = if CONCEPTS.contains(&target) {
                    target
                } else {
                    "main"
                };
                self.targets.insert(target.to_owned());
            }
            Some(_) => {
                for watched in WATCHED_EXTERNAL {
                    let prefix = watched.split("::").collect::<Vec<_>>();
                    if segments.len() >= prefix.len()
                        && segments
                            .iter()
                            .zip(&prefix)
                            .all(|(have, want)| have == want)
                    {
                        self.targets.insert((*watched).to_owned());
                    }
                }
            }
            None => {}
        }
    }
}

impl<'ast> Visit<'ast> for Imports {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.record_use_tree(&node.tree, &mut Vec::new());
    }

    /// `pub(in crate::<concept>)` restricts an item; it does not import one.
    fn visit_visibility(&mut self, _node: &'ast syn::Visibility) {}

    fn visit_path(&mut self, node: &'ast syn::Path) {
        let segments = node
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        self.record(&segments);
        syn::visit::visit_path(self, node);
    }
}

impl Imports {
    fn record_use_tree(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.record_use_tree(&path.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                self.record(prefix);
                prefix.pop();
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                self.record(prefix);
                prefix.pop();
            }
            syn::UseTree::Glob(_) => self.record(prefix),
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.record_use_tree(item, prefix);
                }
            }
        }
    }
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| match &attr.meta {
        syn::Meta::List(list) => {
            list.path.is_ident("cfg") && list.tokens.to_string().replace(' ', "") == "test"
        }
        _ => false,
    })
}
