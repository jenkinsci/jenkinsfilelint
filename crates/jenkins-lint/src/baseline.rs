//! The baseline: findings a repository already had, which do not fail a run.
//! A finding is known by its file, rule and code, so it matches when code moves.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::rules::Finding;

pub const FILE_NAME: &str = ".jenkins-lint-baseline.json";

type Key = (String, String, String);

#[derive(Debug, Default)]
pub struct Baseline {
    pub path: PathBuf,
    root: PathBuf,
    entries: BTreeMap<Key, usize>,
    /// What this run has not matched yet, per entry.
    left: BTreeMap<Key, usize>,
    checked: BTreeSet<String>,
    /// What this run found, for --write-baseline.
    found: BTreeMap<Key, usize>,
}

#[derive(Deserialize)]
struct FileFormat {
    findings: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    file: String,
    rule: String,
    code: String,
    count: usize,
}

/// The nearest baseline file, looking up to the Git root.
pub fn find(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    for dir in start.ancestors() {
        let path = dir.join(FILE_NAME);
        if path.is_file() {
            return Some(path);
        }
        if dir.join(".git").exists() {
            break;
        }
    }
    None
}

/// Where --write-baseline writes by default: the Git root, or here.
pub fn default_path(start: &Path) -> PathBuf {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let root = start.ancestors().find(|d| d.join(".git").exists()).unwrap_or(&start);
    root.join(FILE_NAME)
}

/// The text a finding is about, with whitespace runs made one space.
fn code(text: &str, f: &Finding) -> String {
    let (start, end) = (f.span.start, f.span.end);
    let mut out = String::new();
    for (i, line) in text.lines().enumerate().skip(start.line.saturating_sub(1)) {
        let n = i + 1;
        if n > end.line {
            break;
        }
        let chars: Vec<char> = line.chars().collect();
        let from = if n == start.line { start.col.saturating_sub(1) } else { 0 }.min(chars.len());
        let to = if n == end.line { end.col.saturating_sub(1) } else { chars.len() }.clamp(from, chars.len());
        out.push(' ');
        out.extend(&chars[from..to]);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Baseline {
    /// Read the baseline at `path`. A missing file is an empty baseline.
    pub fn load(path: &Path) -> Result<Baseline, String> {
        let root = path
            .parent()
            .map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
            .unwrap_or_default();
        let mut entries = BTreeMap::new();
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let parsed: FileFormat = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                for e in parsed.findings {
                    *entries.entry((e.file, e.rule, e.code)).or_insert(0) += e.count;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
        Ok(Baseline {
            path: path.to_path_buf(),
            root,
            left: entries.clone(),
            entries,
            ..Baseline::default()
        })
    }

    /// The file's name relative to the baseline, with `/` separators.
    fn file_key(&self, path: &Path) -> String {
        let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let rel = full.strip_prefix(&self.root).unwrap_or(path);
        let name = rel.to_string_lossy().replace('\\', "/");
        name.strip_prefix("./").unwrap_or(&name).to_string()
    }

    /// Split a file's findings into the new ones and those in the baseline.
    pub fn split(&mut self, path: &Path, text: &str, findings: Vec<Finding>) -> (Vec<Finding>, Vec<Finding>) {
        let file = self.file_key(path);
        self.checked.insert(file.clone());
        let (mut new, mut known) = (Vec::new(), Vec::new());
        for f in findings {
            let key = (file.clone(), f.rule.to_string(), code(text, &f));
            *self.found.entry(key.clone()).or_insert(0) += 1;
            match self.left.get_mut(&key) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    known.push(f);
                }
                _ => new.push(f),
            }
        }
        (new, known)
    }

    /// How many findings of the baseline, in the files checked, are gone.
    pub fn fixed(&self) -> usize {
        self.left
            .iter()
            .filter(|((file, _, _), _)| self.checked.contains(file))
            .map(|(_, n)| n)
            .sum()
    }

    /// Write what this run found, keeping files it did not check.
    pub fn write(&self) -> Result<usize, String> {
        let mut all: BTreeMap<&Key, usize> = self
            .entries
            .iter()
            .filter(|((file, _, _), _)| !self.checked.contains(file))
            .map(|(k, n)| (k, *n))
            .collect();
        all.extend(self.found.iter().map(|(k, n)| (k, *n)));
        let lines: Vec<String> = all
            .iter()
            .map(|((file, rule, code), count)| serde_json::json!({"file": file, "rule": rule, "code": code, "count": count}).to_string())
            .collect();
        let text = if lines.is_empty() {
            "{\"version\": 1, \"findings\": []}\n".to_string()
        } else {
            format!("{{\"version\": 1, \"findings\": [\n  {}\n]}}\n", lines.join(",\n  "))
        };
        std::fs::write(&self.path, text).map_err(|e| format!("{}: {e}", self.path.display()))?;
        Ok(all.values().sum())
    }
}
