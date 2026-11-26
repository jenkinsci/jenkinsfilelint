//! Which files to check: the files found in a directory, and the --include
//! and --skip patterns.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// Copies and notes named after a Jenkinsfile. They can hold a whole
/// pipeline, so the content check would pick them.
const NOT_PIPELINE_SUFFIXES: [&str; 10] = [".adoc", ".bak", ".md", ".old", ".orig", ".rej", ".rst", ".swp", ".tmp", ".txt"];
/// Shared library directories: every `*.groovy` under them is checked.
pub const LIBRARY_DIRS: [&str; 2] = ["src", "vars"];
/// Directories of tools. Other hidden directories, like .jenkins, are searched.
const TOOL_DIRS: [&str; 18] = [
    ".cache",
    ".git",
    ".gradle",
    ".hg",
    ".idea",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".svn",
    ".terraform",
    ".tox",
    ".venv",
    ".vscode",
    "__pycache__",
    "node_modules",
    "site",
    "target",
    "venv",
];
/// A build/ directory next to one of these is build output.
const BUILD_FILES: [&str; 6] = [
    "CMakeLists.txt",
    "build.gradle",
    "build.gradle.kts",
    "package.json",
    "pyproject.toml",
    "setup.py",
];

/// The text of `src` with comments and strings blanked out, keeping their
/// line breaks: they can hold text that looks like a pipeline.
fn code_only(src: &[char]) -> Vec<char> {
    let n = src.len();
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| src.get(i + k) == Some(&c));
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let start = i;
        let end = if at(i, "//") {
            let mut j = i + 2;
            while j < n && src[j] != '\n' {
                j += 1;
            }
            Some(j)
        } else if at(i, "/*") {
            let mut j = i + 2;
            while j < n && !at(j, "*/") {
                j += 1;
            }
            Some((j + 2).min(n))
        } else if at(i, "'''") || at(i, "\"\"\"") {
            let quote = src[i];
            let mut j = i + 3;
            while j < n && !(src[j] == quote && at(j, &quote.to_string().repeat(3))) {
                j += if src[j] == '\\' { 2 } else { 1 };
            }
            Some((j + 3).min(n))
        } else if src[i] == '\'' || src[i] == '"' {
            let quote = src[i];
            let mut j = i + 1;
            while j < n && src[j] != quote && src[j] != '\n' {
                j += if src[j] == '\\' { 2 } else { 1 };
            }
            let j = j.min(n);
            Some(if j < n && src[j] == quote { j + 1 } else { j })
        } else if at(i, "$/") {
            let mut j = i + 2;
            while j < n && !at(j, "/$") {
                j += 1;
            }
            Some((j + 2).min(n))
        } else if src[i] == '/' && i > 0 && ("=(,~:[!&|?{};".contains(src[i - 1]) || src[i - 1].is_whitespace()) {
            // A slashy string, when it ends on the same line.
            match src.get(i + 1) {
                Some(c) if *c != '/' && *c != '*' && !c.is_whitespace() => {
                    let mut j = i + 1;
                    while j < n && src[j] != '/' && src[j] != '\n' {
                        j += if src[j] == '\\' { 2 } else { 1 };
                    }
                    (j < n && src[j] == '/').then_some(j + 1)
                }
                _ => None,
            }
        } else {
            None
        };
        match end {
            Some(end) => {
                let end = end.min(n);
                out.push(' ');
                out.extend(src[start..end].iter().filter(|c| **c == '\n'));
                i = end;
            }
            None => {
                out.push(src[i]);
                i += 1;
            }
        }
    }
    out
}

/// Whether the text has a top-level `pipeline {`, `node {` or `node(`.
/// Comments and strings are ignored, and so is anything inside a block, such
/// as a helper method that runs a `node`.
pub fn looks_like_pipeline(text: &str) -> bool {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let chars: Vec<char> = text.chars().collect();
    let mut top = Vec::new();
    let mut depth = 0usize;
    for c in code_only(&chars) {
        match c {
            '(' | '{' | '[' => {
                if depth == 0 {
                    top.push(c);
                }
                depth += 1;
            }
            ')' | '}' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    top.push(c);
                }
            }
            c if depth == 0 => top.push(c),
            _ => {}
        }
    }
    let starts = (0..top.len()).filter(|&i| i == 0 || top[i - 1] == '\n' || top[i - 1] == ';');
    for i in starts {
        let mut j = i;
        while j < top.len() && top[j].is_whitespace() {
            j += 1;
        }
        let word = |w: &str| w.chars().enumerate().all(|(k, c)| top.get(j + k) == Some(&c));
        let after = |len: usize, ends: &[char]| {
            let mut k = j + len;
            while k < top.len() && top[k].is_whitespace() {
                k += 1;
            }
            top.get(k).is_some_and(|c| ends.contains(c))
        };
        if (word("pipeline") && after(8, &['{'])) || (word("node") && after(4, &['(', '{'])) {
            return true;
        }
    }
    false
}

fn read_lossy(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Whether a directory search picks this file: the name finds candidates,
/// and the content confirms those that could be something else.
fn is_pipeline_file(path: &Path, in_library: bool) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if NOT_PIPELINE_SUFFIXES.iter().any(|s| name.ends_with(s)) || !path.is_file() {
        return false;
    }
    if name == "jenkinsfile" || name.ends_with(".jenkinsfile") {
        return true;
    }
    if in_library && name.ends_with(".groovy") {
        return true;
    }
    if name.starts_with("jenkinsfile") || name.ends_with(".groovy") {
        return read_lossy(path).is_some_and(|t| looks_like_pipeline(&t));
    }
    false
}

/// A path written without `.` and with `a/..` removed, as Python's
/// os.path.normpath writes it.
pub fn normpath(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => {
                out.pop();
                depth -= 1;
            }
            Component::Normal(_) => {
                out.push(c);
                depth += 1;
            }
            _ => out.push(c),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// The files to check under `dir`, sorted. Symlinks to directories are not
/// followed.
pub fn find_files(dir: &Path) -> Vec<PathBuf> {
    let root_name = std::fs::canonicalize(dir)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                dirs.push(entry.file_name().to_string_lossy().into_owned());
            } else if kind.is_symlink() && path.is_dir() {
                // A symlink to a directory: not followed.
            } else {
                files.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        let is_output = files.iter().any(|f| BUILD_FILES.contains(&f.as_str()));
        let rel = current.strip_prefix(dir).unwrap_or(&current);
        let in_library = std::iter::once(root_name.as_str())
            .chain(rel.components().filter_map(|c| c.as_os_str().to_str()))
            .any(|p| LIBRARY_DIRS.contains(&p));
        for name in files {
            let path = normpath(&current.join(&name));
            if is_pipeline_file(&path, in_library) {
                found.push(path);
            }
        }
        for name in dirs {
            if !(TOOL_DIRS.contains(&name.as_str()) || name == "build" && is_output) {
                stack.push(current.join(name));
            }
        }
    }
    found.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    found
}

/// Each directory in `paths` replaced with the files found in it; other
/// paths stay as they are. A file also given by name, or found through
/// another directory, is checked once. Also returns the directories in
/// which no file was found.
pub fn expand_paths(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut seen: HashSet<PathBuf> = paths.iter().filter(|p| !p.is_dir()).map(|p| normpath(p)).collect();
    let mut files = Vec::new();
    let mut empty = Vec::new();
    for path in paths {
        if !path.is_dir() {
            files.push(path.clone());
            continue;
        }
        let found = find_files(path);
        if found.is_empty() {
            empty.push(path.clone());
        }
        for f in found {
            if seen.insert(f.clone()) {
                files.push(f);
            }
        }
    }
    (files, empty)
}

/// fnmatch of one path segment: `*`, `?`, `[abc]` and `[!abc]`.
fn fnmatch(name: &[char], pat: &[char]) -> bool {
    let (mut n, mut p) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pat.len() {
            match pat[p] {
                '*' => {
                    star = Some((p, n));
                    p += 1;
                    continue;
                }
                '?' => {
                    n += 1;
                    p += 1;
                    continue;
                }
                '[' => {
                    if let Some((matched, len)) = class(&pat[p..], name[n]) {
                        if matched {
                            n += 1;
                            p += len;
                            continue;
                        }
                    } else if name[n] == '[' {
                        n += 1;
                        p += 1;
                        continue;
                    }
                }
                c if c == name[n] => {
                    n += 1;
                    p += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((sp, sn)) => {
                p = sp + 1;
                n = sn + 1;
                star = Some((sp, sn + 1));
            }
            None => return false,
        }
    }
    pat[p..].iter().all(|c| *c == '*')
}

/// A `[...]` class at the start of `pat`: whether `c` is in it, and its
/// length; None when it does not close, so that `[` is literal.
fn class(pat: &[char], c: char) -> Option<(bool, usize)> {
    let mut i = 1;
    let negate = pat.get(i).is_some_and(|x| *x == '!');
    if negate {
        i += 1;
    }
    let first = i;
    let mut matched = false;
    while i < pat.len() && (pat[i] != ']' || i == first) {
        if pat.get(i + 1) == Some(&'-') && pat.get(i + 2).is_some_and(|x| *x != ']') {
            matched |= pat[i] <= c && c <= pat[i + 2];
            i += 3;
        } else {
            matched |= pat[i] == c;
            i += 1;
        }
    }
    (i < pat.len()).then_some((matched != negate, i + 1))
}

fn parts(path: &str) -> Vec<String> {
    let path = Path::new(path);
    let mut out: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            Component::RootDir | Component::Prefix(_) => out.push(String::from("/")),
            Component::CurDir => {}
            c => out.push(c.as_os_str().to_string_lossy().into_owned()),
        }
    }
    out
}

/// Whether `path` matches `pattern` as Python's PurePath.match does: from
/// the right, one segment for each segment of the pattern, or the whole
/// path for an absolute pattern.
pub fn path_match(path: &str, pattern: &str) -> bool {
    let path_parts = parts(path);
    let pat_parts = parts(pattern);
    if pat_parts.is_empty() {
        return false;
    }
    let absolute = Path::new(pattern).has_root();
    if absolute && pat_parts.len() != path_parts.len() || pat_parts.len() > path_parts.len() {
        return false;
    }
    let fold = |s: &str| -> Vec<char> {
        if cfg!(windows) {
            s.to_lowercase().chars().collect()
        } else {
            s.chars().collect()
        }
    };
    path_parts
        .iter()
        .rev()
        .zip(pat_parts.iter().rev())
        .all(|(part, pat)| part == pat && pat == "/" || fnmatch(&fold(part), &fold(pat)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "pipeline {\n  agent any\n  stages { stage('a') { steps { echo 'a' } } }\n}\n";
    const INVALID: &str = "pipeline {\n  agent any\n}\n";
    const SCRIPTED: &str = "node {\n    sh 'make'\n}\n";
    const HELPER: &str = "def greet(name) {\n    echo name\n}\n";

    #[test]
    fn pipelines() {
        for text in [
            VALID,
            SCRIPTED,
            "node('linux') {\n    sh 'make'\n}\n",
            "node ('linux') {\n}\n",
            "@Library('lib@v1') _\n\npipeline {\n}\n",
            "#!/usr/bin/env groovy\n// a comment\nnode {\n}\n",
            "/* a comment */ pipeline {\n}\n",
            "/* a\n comment */\npipeline {\n}\n",
            "\u{feff}pipeline {\n}\n",
            "import groovy.json.JsonSlurper\ndef v = 'a'\nnode {\n}\n",
            "def v = 1; node {\n}\n",
            "def helper() {\n    echo 'a'\n}\nnode {\n}\n",
            "def p = ~/a{2}/\nnode {\n}\n",
            "}\nnode {\n}\n",
        ] {
            assert!(looks_like_pipeline(text), "{text:?}");
        }
        for text in [
            HELPER,
            "",
            "// pipeline {\n",
            "/*\npipeline {\n}\n*/\n",
            "def a = 'pipeline {'\n",
            "def a = \"node {\"\n",
            "def a = '''\npipeline {\n}\n'''\n",
            "def a = \"\"\"\nnode {\n}\n\"\"\"\n",
            "def a = $/\nnode {\n}\n/$\n",
            "def a = /pipeline {/\n",
            "def call() {\n    pipeline {\n    }\n}\n",
            "def call() {\n    node {\n    }\n}\n",
            "def node(String label) {\n    echo label\n}\n",
            "String node = 'a'\n",
            "helper.node {\n}\n",
            "pipelines {\n}\n",
        ] {
            assert!(!looks_like_pipeline(text), "{text:?}");
        }
    }

    fn write(root: &Path, names: &[&str], text: &str) {
        for name in names {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    }

    fn found(root: &Path) -> Vec<String> {
        find_files(root)
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn names() {
        for (name, text, picked) in [
            ("Jenkinsfile", INVALID, true),
            ("Jenkinsfile.prod", VALID, true),
            ("deploy.jenkinsfile", INVALID, true),
            ("jenkinsfile", VALID, true),
            ("JenkinsFile", VALID, true),
            ("Jenkinsfile-prod", VALID, true),
            ("Jenkinsfile_prod", VALID, true),
            ("ci/deploy.groovy", VALID, true),
            ("pipelines/release.groovy", SCRIPTED, true),
            (".jenkins/Jenkinsfile", VALID, true),
            ("build/Jenkinsfile", VALID, true),
            ("jenkins/build.pipeline", VALID, false),
            ("ci/deploy.jenkins", VALID, false),
            ("Jenkinsfile.md", VALID, false),
            ("Jenkinsfile.bak", VALID, false),
            ("Jenkinsfile.dockerfile", "FROM alpine\n", false),
            ("scripts/helper.groovy", HELPER, false),
            ("Jenkinsfile-notes", HELPER, false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), &[name], text);
            let want: Vec<String> = if picked { vec![name.to_string()] } else { vec![] };
            assert_eq!(found(dir.path()), want, "{name}");
        }
    }

    #[test]
    fn library_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["Jenkinsfile"], VALID);
        write(
            dir.path(),
            &[
                "vars/deploy.groovy",
                "src/com/acme/Util.groovy",
                "scripts/other.groovy",
                "vars/notes.txt",
            ],
            HELPER,
        );
        assert_eq!(found(dir.path()), ["Jenkinsfile", "src/com/acme/Util.groovy", "vars/deploy.groovy"]);
        // Searching inside a library directory, but not below a parent of it.
        assert_eq!(found(&dir.path().join("vars")), ["deploy.groovy"]);
        write(dir.path(), &["src/project/tools/helper.groovy"], HELPER);
        assert_eq!(found(&dir.path().join("src/project")), Vec::<String>::new());
    }

    #[test]
    fn skipped_directories() {
        for d in [".git", ".venv", ".tox", "venv", "node_modules", "target", "site"] {
            let dir = tempfile::tempdir().unwrap();
            let names = ["Jenkinsfile".to_string(), format!("{d}/Jenkinsfile"), format!("a/{d}/Jenkinsfile")];
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            write(dir.path(), &names, VALID);
            assert_eq!(found(dir.path()), ["Jenkinsfile"], "{d}");
            // Asked for by name, it is searched.
            assert_eq!(found(&dir.path().join(d)), ["Jenkinsfile"], "{d}");
        }
        for build_file in ["build.gradle", "pyproject.toml"] {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), &["Jenkinsfile", "build/Jenkinsfile"], VALID);
            write(dir.path(), &[build_file], "");
            assert_eq!(found(dir.path()), ["Jenkinsfile"], "{build_file}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["real/Jenkinsfile"], VALID);
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("Jenkinsfile.broken")).unwrap();
        assert_eq!(found(dir.path()), ["real/Jenkinsfile"]);
    }

    #[test]
    fn expanding_paths() {
        let paths = [
            PathBuf::from("./Jenkinsfile"),
            PathBuf::from("missing"),
            PathBuf::from("./Jenkinsfile"),
        ];
        assert_eq!(expand_paths(&paths), (paths.to_vec(), vec![]));
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &["Jenkinsfile", "ci/Jenkinsfile"], VALID);
        let given = dir.path().join("Jenkinsfile");
        let (files, empty) = expand_paths(&[dir.path().to_path_buf(), given.clone(), dir.path().join("ci")]);
        // Found once, and the one also given by name only where it is named.
        assert_eq!(files, [normpath(&dir.path().join("ci/Jenkinsfile")), given]);
        assert!(empty.is_empty());
        let none = tempfile::tempdir().unwrap();
        assert_eq!(
            expand_paths(&[none.path().to_path_buf()]),
            (vec![], vec![none.path().to_path_buf()])
        );
    }

    #[test]
    fn globs() {
        assert!(path_match("a/vars/x.groovy", "vars/*.groovy"));
        assert!(path_match("vars/x.groovy", "*.groovy"));
        assert!(!path_match("x.groovy", "vars/*.groovy"));
        assert!(path_match("ci/Jenkinsfile-prod", "Jenkinsfile*"));
        assert!(!path_match("src/Utils.groovy", "*/src/*.groovy"));
        assert!(path_match("a/src/Utils.groovy", "*/src/*.groovy"));
        assert!(path_match("Jenkinsfile.1", "Jenkinsfile.[0-9]"));
        assert!(!path_match("Jenkinsfile.a", "Jenkinsfile.[!a-z]"));
        assert!(path_match("/abs/Jenkinsfile", "/abs/*"));
        assert!(!path_match("rel/abs/Jenkinsfile", "/abs/*"));
        assert!(!path_match("anything", ""));
    }

    #[test]
    fn normpaths() {
        assert_eq!(normpath(Path::new("./a/./b")), PathBuf::from("a/b"));
        assert_eq!(normpath(Path::new("a/../b")), PathBuf::from("b"));
        assert_eq!(normpath(Path::new("..")), PathBuf::from(".."));
        assert_eq!(normpath(Path::new(".")), PathBuf::from("."));
    }
}
