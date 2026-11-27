//! --remote: validate files with a Jenkins server's
//! /pipeline-model-converter/validate endpoint.

use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig};
use ureq::Agent;

// The endpoint answers in plain text with one of these fixed shapes
// (ModelConverterAction#doValidate in the pipeline-model-definition plugin).
const VALIDATED_PREFIX: &str = "Jenkinsfile successfully validated";
const ERRORS_PREFIX: &str = "Errors encountered validating Jenkinsfile:";
const NO_JENKINSFILE: &str = "No Jenkinsfile specified";
const COMPILE_ERROR_PREFIX: &str = "WorkflowScript:";
// Sent when the Groovy parses but has no top-level `pipeline` block. Jenkins
// echoes the whole file between the quotes, so only the ends are matched.
const NOT_DECLARATIVE_PREFIX: &str = "Jenkinsfile content '";
const NOT_DECLARATIVE_SUFFIX: &str = "' did not contain the 'pipeline' step";

/// What Jenkins said about one file.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Valid(String),
    NotDeclarative,
    Invalid(String),
}

/// Map a plain-text validate response to a verdict. The file content that
/// Jenkins echoes back is never searched or returned.
pub fn parse_text(text: &str) -> Verdict {
    let result = text.trim();
    if result.starts_with(NOT_DECLARATIVE_PREFIX) && result.ends_with(NOT_DECLARATIVE_SUFFIX) {
        return Verdict::NotDeclarative;
    }
    if result.starts_with(VALIDATED_PREFIX) {
        return Verdict::Valid(result.to_string());
    }
    if [ERRORS_PREFIX, NO_JENKINSFILE, COMPILE_ERROR_PREFIX]
        .iter()
        .any(|p| result.starts_with(p))
    {
        return Verdict::Invalid(result.to_string());
    }
    // Anything else (an HTML login page from a proxy, an empty body, ...)
    // means the file was not validated, so it is not reported as valid.
    let first: String = result.lines().next().unwrap_or("").chars().take(200).collect();
    let first = if result.is_empty() { "<empty response>".to_string() } else { first };
    Verdict::Invalid(format!("Unexpected response from Jenkins validate endpoint: {first}"))
}

/// A JSON answer, which some Jenkins versions and proxies send.
fn parse_json(text: &str) -> Option<Verdict> {
    let t = text.trim();
    if !t.starts_with('{') {
        return None;
    }
    let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("\"status\":\"ok\"") {
        return Some(Verdict::Valid("Jenkinsfile successfully validated".to_string()));
    }
    Some(Verdict::Invalid(format!("Validation errors:\n{t}")))
}

fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A client for one Jenkins server, shared by the threads that send files.
pub struct Jenkins {
    url: String,
    auth: Option<String>,
    agent: Agent,
}

impl Jenkins {
    pub fn new(url: &str, username: Option<&str>, token: Option<&str>) -> Self {
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            .user_agent(concat!("jenkins-lint/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let auth = match (username, token) {
            (Some(u), Some(t)) if !u.is_empty() && !t.is_empty() => Some(format!("Basic {}", base64(format!("{u}:{t}").as_bytes()))),
            _ => None,
        };
        Jenkins {
            url: format!("{}/pipeline-model-converter/validate", url.trim_end_matches('/')),
            auth,
            agent,
        }
    }

    /// Validate one file's text.
    pub fn validate(&self, source: &str) -> Verdict {
        let mut request = self.agent.post(&self.url);
        if let Some(auth) = &self.auth {
            request = request.header("Authorization", auth);
        }
        let response = match request.send_form([("jenkinsfile", source)]) {
            Ok(r) => r,
            Err(e) => return Verdict::Invalid(format!("Error connecting to Jenkins: {e}")),
        };
        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            let kind = if status.is_client_error() { "Client" } else { "Server" };
            return Verdict::Invalid(format!(
                "Error connecting to Jenkins: {} {kind} Error: {} for url: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                self.url
            ));
        }
        match response.into_body().read_to_string() {
            Ok(body) => parse_json(&body).unwrap_or_else(|| parse_text(&body)),
            Err(e) => Verdict::Invalid(format!("Error connecting to Jenkins: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_responses() {
        assert_eq!(
            parse_text("Jenkinsfile successfully validated.\n"),
            Verdict::Valid("Jenkinsfile successfully validated.".into())
        );
        assert_eq!(
            parse_text("Jenkinsfile content 'node { error \"x\" }' did not contain the 'pipeline' step"),
            Verdict::NotDeclarative
        );
        assert!(
            matches!(parse_text("Errors encountered validating Jenkinsfile:\nWorkflowScript: 2: x"), Verdict::Invalid(m) if m.starts_with("Errors"))
        );
        assert_eq!(
            parse_text("<html><body>Login</body></html>"),
            Verdict::Invalid("Unexpected response from Jenkins validate endpoint: <html><body>Login</body></html>".into())
        );
        assert_eq!(
            parse_text("  "),
            Verdict::Invalid("Unexpected response from Jenkins validate endpoint: <empty response>".into())
        );
        assert_eq!(
            parse_json("{\"status\": \"ok\"}"),
            Some(Verdict::Valid("Jenkinsfile successfully validated".into()))
        );
        assert_eq!(parse_json("plain"), None);
    }

    #[test]
    fn basic_auth() {
        assert_eq!(base64(b"user:token"), "dXNlcjp0b2tlbg==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }
}
