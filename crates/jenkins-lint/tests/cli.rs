//! The jenkins-lint command, run as users run it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

const VALID: &str = "pipeline {\n  agent any\n  stages { stage('a') { steps { echo 'a' } } }\n}\n";
const INVALID: &str = "pipeline {\n  agent\n  stages { stage('a') { steps { echo 'a' } } }\n}\n";
const SCRIPTED: &str = "node {\n    sh 'make'\n}\n";
const HELPER: &str = "def greet(name) {\n    echo name\n}\n";
const LEAK: &str = "withCredentials([string(credentialsId: 'c', variable: 'TOKEN')]) {\n    sh \"deploy --token ${TOKEN}\"\n}\n";
const INJECTION: &str = "pipeline {\n  agent any\n  stages { stage('a') { steps {\n    sh \"echo ${env.CHANGE_TITLE}\"\n  } } }\n}\n";
const NOTE: &str = "ℹ Checking offline: step names, step parameters and other plugin-dependent checks are skipped. \
     Pass --remote to also check with your Jenkins.";

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_jenkins-lint"));
    c.current_dir(dir);
    for name in ["JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN"] {
        c.env_remove(name);
    }
    c
}

fn done(o: Output) -> Run {
    Run {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn run(dir: &Path, args: &[&str]) -> Run {
    done(command(dir).args(args).output().unwrap())
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

/// A stand-in for Jenkins's validate endpoint: answers like Jenkins does,
/// and records the requests.
/// The Authorization header and the file of each request.
type Requests = Arc<Mutex<Vec<(Option<String>, String)>>>;

struct FakeJenkins {
    url: String,
    requests: Requests,
}

fn fake_jenkins(status: u16) -> FakeJenkins {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                loop {
                    let mut auth = None;
                    let mut length = 0;
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    loop {
                        let mut header = String::new();
                        reader.read_line(&mut header).unwrap();
                        let header = header.trim_end();
                        if header.is_empty() {
                            break;
                        }
                        let (name, value) = header.split_once(':').unwrap();
                        match name.to_ascii_lowercase().as_str() {
                            "content-length" => length = value.trim().parse().unwrap(),
                            "authorization" => auth = Some(value.trim().to_string()),
                            _ => {}
                        }
                    }
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    let form = String::from_utf8(body).unwrap();
                    let source = decode(form.strip_prefix("jenkinsfile=").unwrap_or(""));
                    seen.lock().unwrap().push((auth, source.clone()));
                    let text = if source.trim_start().starts_with("pipeline") {
                        "Jenkinsfile successfully validated.".to_string()
                    } else {
                        format!("Jenkinsfile content '{source}' did not contain the 'pipeline' step")
                    };
                    let reason = if status == 200 { "OK" } else { "Unauthorized" };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{text}",
                        text.len()
                    );
                    reader.get_mut().write_all(response.as_bytes()).unwrap();
                }
            });
        }
    });
    FakeJenkins { url, requests }
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn a_valid_file_passes_quietly() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    let r = run(dir.path(), &["Jenkinsfile"]);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "", ""));
}

#[test]
fn errors_and_findings_in_one_run() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", &INJECTION.replace("agent any", "agent"));
    for command in [&[][..], &["check"], &["audit"], &["--audit"]] {
        let r = run(dir.path(), &[command, &["Jenkinsfile"]].concat());
        assert_eq!(r.code, 1);
        assert!(
            r.err.starts_with("Jenkinsfile:2:3: error: Not a valid section definition"),
            "{}",
            r.err
        );
        assert!(
            r.out.starts_with("Jenkinsfile:4:14: high: script-injection: CHANGE_TITLE"),
            "{}",
            r.out
        );
        assert!(r
            .out
            .contains("\n  help: https://jenkinsci.github.io/jenkins-lint/audit/script-injection/\n"));
        assert!(r
            .out
            .ends_with("1 finding (1 high)\n1 fixable with --fix --unsafe-fixes (they can change what the pipeline does, review them).\n"));
    }
}

#[test]
fn several_files_are_named() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    write(dir.path(), "Jenkinsfile.fail", INVALID);
    write(dir.path(), "scripted.groovy", SCRIPTED);
    let r = run(dir.path(), &["Jenkinsfile", "Jenkinsfile.fail", "scripted.groovy"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        r.out,
        "✓ Jenkinsfile: Valid\n⊘ scripted.groovy: Skipped (not a Declarative Pipeline)\n"
    );
    assert!(r.err.starts_with("Jenkinsfile.fail:2:3: error: "));
}

#[test]
fn verbose() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    write(dir.path(), "vars/step.groovy", HELPER);
    write(dir.path(), "empty/README.md", "");
    let r = run(dir.path(), &["-v", "--skip", "vars/*", ".", "empty"]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.out,
        format!(
            "ℹ No files to check found in empty\n{NOTE}\n✓ Jenkinsfile: Valid\n⊘ {}: Skipped (matches skip pattern)\n",
            Path::new("vars").join("step.groovy").display()
        )
    );
}

#[test]
fn shared_library_files_are_checked() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    write(dir.path(), "vars/deploy.groovy", LEAK);
    write(dir.path(), "vars/broken.groovy", "def x = {\n");
    write(dir.path(), "vars/fine.groovy", HELPER);
    let r = run(dir.path(), &["."]);
    assert_eq!(r.code, 1);
    let broken = Path::new("vars").join("broken.groovy");
    let deploy = Path::new("vars").join("deploy.groovy");
    assert!(r.err.starts_with(&format!("{}:", broken.display())), "{}", r.err);
    assert!(
        r.out
            .contains(&format!("{}:2:24: high: secret-interpolation: TOKEN", deploy.display())),
        "{}",
        r.out
    );
    // A library file is never a pipeline: not named without -v.
    assert!(!r.out.contains("fine.groovy"), "{}", r.out);
    assert!(r.out.starts_with("✓ Jenkinsfile: Valid\n"), "{}", r.out);
}

#[test]
fn include_and_skip() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    write(dir.path(), "src/Utils.groovy", "def x = {\n");
    write(dir.path(), "ci/Jenkinsfile.prod", INVALID);
    let r = run(dir.path(), &["--include", "Jenkinsfile*", "--skip", "ci/*", "."]);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (0, "✓ Jenkinsfile: Valid\n", ""));
}

#[test]
fn nothing_to_check() {
    let dir = tempfile::tempdir().unwrap();
    let r = run(dir.path(), &["."]);
    assert_eq!((r.code, r.out.as_str()), (0, ""));
    let r = run(dir.path(), &["--format", "sarif", "."]);
    assert_eq!(r.code, 0);
    assert!(r.out.starts_with("{\"$schema\""));
}

#[test]
fn unreadable_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Jenkinsfile"), b"pipeline { agent \xff }").unwrap();
    let r = run(dir.path(), &["Jenkinsfile", "missing"]);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("Error reading file Jenkinsfile: "), "{}", r.err);
    assert!(r.err.contains("Error reading file missing: "), "{}", r.err);
}

#[test]
fn security_options() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Jenkinsfile",
        "withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'USER', passwordVariable: 'PASS')]) {\n    sh \"curl -u ${USER}:${PASS} https://example.com\"\n}\n",
    );
    let r = run(dir.path(), &["--persona", "pedantic", "Jenkinsfile"]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("2 findings (1 high, 1 low)"), "{}", r.out);
    let r = run(dir.path(), &["--persona", "pedantic", "--min-severity", "high", "Jenkinsfile"]);
    assert!(r.out.contains("1 finding (1 high)"), "{}", r.out);
    let r = run(dir.path(), &["--ignore", "secret-interpolation", "Jenkinsfile"]);
    assert_eq!((r.code, r.out.as_str()), (0, ""));
}

#[test]
fn fixes() {
    let dir = tempfile::tempdir().unwrap();
    let f = write(dir.path(), "Jenkinsfile", LEAK);
    // --fix alone makes only safe fixes, and this one is unsafe.
    let r = run(dir.path(), &["--fix", "Jenkinsfile"]);
    assert_eq!(r.code, 1);
    assert!(r.out.ends_with(
        "Found 1 finding (0 fixed, 1 remaining).\n1 fixable with --fix --unsafe-fixes (they can change what the pipeline does, review them).\n"
    ));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), LEAK);
    let r = run(dir.path(), &["--diff", "--unsafe-fixes", "Jenkinsfile"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out
            .contains("-    sh \"deploy --token ${TOKEN}\"\n+    sh 'deploy --token \"$TOKEN\"'\n"),
        "{}",
        r.out
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap(), LEAK);
    for flags in [&["--fix", "--unsafe-fixes"][..], &["--fix=all"]] {
        std::fs::write(&f, LEAK).unwrap();
        let r = run(dir.path(), &[flags, &["Jenkinsfile"]].concat());
        assert_eq!(r.code, 0, "{}", r.out);
        assert!(r.out.starts_with("Jenkinsfile:2:24: fixed: secret-interpolation: TOKEN"));
        assert!(r.out.ends_with("Found 1 finding (1 fixed, 0 remaining).\n"));
        assert!(std::fs::read_to_string(&f).unwrap().contains("sh 'deploy --token \"$TOKEN\"'"));
    }
}

#[test]
fn reports() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", &INJECTION.replace("agent any", "agent"));
    let r = run(dir.path(), &["--format", "json", "Jenkinsfile"]);
    assert_eq!(r.code, 1);
    assert!(
        r.out
            .starts_with("[{\"file\":\"Jenkinsfile\",\"declarative\":true,\"diagnostics\":[{\"line\":2"),
        "{}",
        r.out
    );
    assert!(r.out.contains("\"findings\":[{\"rule\":\"script-injection\""), "{}", r.out);
    let r = run(dir.path(), &["--format", "sarif", "Jenkinsfile"]);
    assert_eq!(r.code, 0);
    assert!(r.out.contains("\"ruleId\":\"invalid-pipeline\""));
    assert!(r.out.contains("\"ruleId\":\"script-injection\""));
}

#[test]
fn usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    for (args, message) in [
        (&[][..], "the following arguments are required: PATH"),
        (&["--bogus", "J"], "unrecognized arguments: --bogus"),
        (&["--username", "u", "J"], "argument --username: requires --remote"),
        (&["--token", "t", "J"], "argument --token: requires --remote"),
        (&["--remote", "J"], "argument --remote: no URL given and JENKINS_URL is not set"),
        (
            &["--offline", "--remote", "http://j", "J"],
            "argument --offline: not allowed with argument --remote",
        ),
        (
            &["--format", "sarif", "--remote", "http://j", "J"],
            "argument --format: not allowed with argument --remote",
        ),
        (
            &["--diff", "--remote", "http://j", "J"],
            "argument --diff: not allowed with argument --remote",
        ),
        (
            &["--diff", "--format", "json", "J"],
            "argument --diff: not allowed with argument --format",
        ),
        (&["--unsafe-fixes", "J"], "argument --unsafe-fixes: requires --fix or --diff"),
        (&["--fix=some", "J"], "argument --fix: invalid choice: 'some'"),
        (&["--persona", "everyone", "J"], "argument --persona: invalid choice: 'everyone'"),
        (
            &["--ignore", "no-such-rule", "J"],
            "argument --ignore: unknown rule \"no-such-rule\"",
        ),
        (&["-j", "0", "J"], "argument -j/--jobs: must be at least 1"),
        (&["--jobs", "x", "J"], "argument -j/--jobs: invalid int value: 'x'"),
        (&["--local", "J"], "--local was removed in jenkins-lint 2.0"),
    ] {
        let r = run(dir.path(), args);
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.err.contains(message), "{args:?}: {}", r.err);
    }
    let r = run(dir.path(), &["server", "stop"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("--local was removed"));
}

#[test]
fn help_and_version() {
    let dir = tempfile::tempdir().unwrap();
    let r = run(dir.path(), &["--version"]);
    assert_eq!((r.code, r.out.trim()), (0, concat!("jenkins-lint ", env!("CARGO_PKG_VERSION"))));
    let r = run(dir.path(), &["--help"]);
    assert_eq!(r.code, 0);
    assert!(r.out.contains("Usage: jenkins-lint [check] [OPTIONS] <PATH>..."));
}

#[test]
fn files_after_double_dash() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "--fix", VALID);
    let r = run(dir.path(), &["--", "--fix"]);
    assert_eq!(r.code, 0, "{}", r.err);
}

#[test]
fn the_environment_alone_stays_offline() {
    let jenkins = fake_jenkins(200);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    let r = done(
        command(dir.path())
            .env("JENKINS_URL", &jenkins.url)
            .env("JENKINS_USER", "u")
            .env("JENKINS_TOKEN", "t")
            .arg("Jenkinsfile")
            .output()
            .unwrap(),
    );
    assert_eq!(r.code, 0);
    assert!(jenkins.requests.lock().unwrap().is_empty());
}

#[test]
fn remote_sends_what_passes_offline() {
    let jenkins = fake_jenkins(200);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    write(dir.path(), "Jenkinsfile.fail", INVALID);
    write(dir.path(), "scripted.groovy", SCRIPTED);
    let r = run(
        dir.path(),
        &["--remote", &jenkins.url, "-v", "Jenkinsfile", "Jenkinsfile.fail", "scripted.groovy"],
    );
    assert_eq!(r.code, 1);
    assert!(r.err.starts_with("Jenkinsfile.fail:2:3: error: "), "{}", r.err);
    assert_eq!(
        r.out,
        "⊘ scripted.groovy: Skipped (not a Declarative Pipeline)\nValidating Jenkinsfile...\n✓ Jenkinsfile: Valid\n  Jenkinsfile successfully validated.\n"
    );
    let requests = jenkins.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0], (None, VALID.to_string()));
}

#[test]
fn bare_remote_and_credentials_from_the_environment() {
    let jenkins = fake_jenkins(200);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    let r = done(
        command(dir.path())
            .env("JENKINS_URL", &jenkins.url)
            .env("JENKINS_USER", "user")
            .env("JENKINS_TOKEN", "token")
            .args(["--remote", "Jenkinsfile"])
            .output()
            .unwrap(),
    );
    assert_eq!(r.code, 0, "{}", r.err);
    let requests = jenkins.requests.lock().unwrap();
    assert_eq!(requests[0].0.as_deref(), Some("Basic dXNlcjp0b2tlbg=="));
}

#[test]
fn a_refusal_is_printed_once_and_never_shows_the_token() {
    let jenkins = fake_jenkins(401);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    write(dir.path(), "Jenkinsfile.prod", VALID);
    let r = run(
        dir.path(),
        &[
            "--remote",
            &jenkins.url,
            "--username",
            "u",
            "--token",
            "secret-token",
            "Jenkinsfile",
            "Jenkinsfile.prod",
        ],
    );
    assert_eq!(r.code, 1);
    assert_eq!(r.err.matches("401 Client Error: Unauthorized").count(), 1, "{}", r.err);
    assert!(!r.err.contains("secret-token") && !r.out.contains("secret-token"));
}

#[test]
fn jobs_keep_the_output_in_order() {
    let jenkins = fake_jenkins(200);
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<String> = (0..12).map(|i| format!("Jenkinsfile.{i:02}")).collect();
    for name in &names {
        write(dir.path(), name, VALID);
    }
    let mut args = vec!["--remote", &jenkins.url, "-j", "4"];
    args.extend(names.iter().map(String::as_str));
    let r = run(dir.path(), &args);
    assert_eq!(r.code, 0, "{}", r.err);
    let want: String = names.iter().map(|n| format!("✓ {n}: Valid\n")).collect();
    assert_eq!(r.out, want);
    assert_eq!(jenkins.requests.lock().unwrap().len(), 12);
}

#[test]
fn cannot_reach_jenkins() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Jenkinsfile", VALID);
    let r = run(dir.path(), &["--remote", &url, "Jenkinsfile"]);
    assert_eq!(r.code, 1);
    assert!(r.err.starts_with("✗ Jenkinsfile: Error connecting to Jenkins: "), "{}", r.err);
}
