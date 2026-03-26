// Licensed to Elasticsearch B.V. under one or more contributor
// license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright
// ownership. Elasticsearch B.V. licenses this file to you under
// the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use assert_cmd::Command;
use wiremock::matchers::{body_string, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// --- helpers -----------------------------------------------------------------

fn escli(server: &MockServer) -> Command {
    let mut cmd = Command::cargo_bin("escli").unwrap();
    cmd.args(["--url", &server.uri()]);
    cmd
}

// --- response handling -------------------------------------------------------

#[tokio::test]
async fn success_response_goes_to_stdout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"status":"ok"}"#))
        .mount(&server)
        .await;

    escli(&server)
        .arg("info")
        .assert()
        .success()
        .stdout(r#"{"status":"ok"}"#);
}

#[tokio::test]
async fn error_response_goes_to_stderr_and_exits_1() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(404).set_body_string(r#"{"error":"not found"}"#),
        )
        .mount(&server)
        .await;

    escli(&server)
        .arg("info")
        .assert()
        .failure()
        .code(1)
        .stderr(r#"{"error":"not found"}"#)
        .stdout("");
}

// --- dispatch ----------------------------------------------------------------

#[tokio::test]
async fn info_command_sends_get_to_root() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server).arg("info").assert().success();

    server.verify().await;
}

// --- authentication ----------------------------------------------------------

#[tokio::test]
async fn api_key_auth_sends_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["--api-key", "myapikey", "info"])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn basic_auth_sends_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["--username", "foo", "--password", "bar", "info"])
        .assert()
        .success();

    server.verify().await;
}

// --- environment variables ---------------------------------------------------

#[tokio::test]
async fn url_from_env_var() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("escli")
        .unwrap()
        .env("ESCLI_URL", server.uri())
        .arg("info")
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn api_key_from_env_var() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("escli")
        .unwrap()
        .env("ESCLI_URL", server.uri())
        .env("ESCLI_API_KEY", "myapikey")
        .arg("info")
        .assert()
        .success();

    server.verify().await;
}

// --- platform-specific -------------------------------------------------------

/// On Windows the Console API can silently convert LF → CRLF when stdout is
/// connected to a console, but when piped (as in tests) the bytes must be
/// written as-is so that JSON stays valid.
#[cfg(windows)]
#[tokio::test]
async fn windows_response_body_has_no_crlf() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"a\":1\n}"))
        .mount(&server)
        .await;

    let assert = escli(&server).arg("info").assert().success();
    let stdout = &assert.get_output().stdout;
    assert!(
        !stdout.windows(2).any(|w| w == b"\r\n"),
        "stdout contains CRLF: {:?}",
        stdout
    );
}

/// On Unix, writing to a closed pipe (e.g. `escli info | head -c 0`) must not
/// print "Error writing to stdout" — the BrokenPipe error should be swallowed.
#[cfg(unix)]
#[tokio::test]
async fn unix_broken_pipe_is_silent() {
    use std::process::Stdio;

    let server = MockServer::start().await;
    // Return enough data that the write is likely to hit the broken pipe.
    let body = "x".repeat(1 << 16);
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let bin = assert_cmd::cargo::cargo_bin("escli");
    let mut child = std::process::Command::new(bin)
        .args(["--url", &server.uri(), "info"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Drop the read end of stdout immediately to induce EPIPE.
    drop(child.stdout.take());

    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Error writing to stdout"),
        "unexpected error on stderr: {stderr}"
    );
}

// --- path parameters ---------------------------------------------------------

#[tokio::test]
async fn path_parameter_is_interpolated_into_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/my-index"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["indices", "get", "my-index"])
        .assert()
        .success();

    server.verify().await;
}

// --- query string ------------------------------------------------------------

#[tokio::test]
async fn query_string_param_is_forwarded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/my-index"))
        .and(query_param("flat_settings", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["indices", "get", "my-index", "--flat_settings", "true"])
        .assert()
        .success();

    server.verify().await;
}

// --- request body ------------------------------------------------------------

#[tokio::test]
async fn body_is_sent_from_stdin() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_create/1"))
        .and(body_string(r#"{"foo":"bar"}"#))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["core", "create", "my-index", "1"])
        .write_stdin(r#"{"foo":"bar"}"#)
        .assert()
        .success();

    server.verify().await;
}

// --- .env file ---------------------------------------------------------------

#[tokio::test]
async fn dotenv_file_is_loaded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        format!("ESCLI_URL={}\n", server.uri()),
    )
    .unwrap();

    Command::cargo_bin("escli")
        .unwrap()
        .current_dir(dir.path())
        .arg("info")
        .assert()
        .success();

    server.verify().await;
}

// --- connection errors -------------------------------------------------------

/// Port 1 is privileged and never listening; this reliably triggers ECONNREFUSED.
#[test]
fn connection_refused_shows_friendly_message() {
    let output = Command::cargo_bin("escli")
        .unwrap()
        .args(["--url", "http://127.0.0.1:1", "info"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "stderr must not be empty on connection error");
    assert!(
        stderr.contains("Could not connect"),
        "expected friendly message, got: {stderr}"
    );
}

// --- argument validation -----------------------------------------------------

#[test]
fn missing_url_fails() {
    Command::cargo_bin("escli")
        .unwrap()
        .arg("info")
        .assert()
        .failure();
}

#[test]
fn username_without_password_fails() {
    Command::cargo_bin("escli")
        .unwrap()
        .args(["--url", "http://localhost:9200", "--username", "foo", "info"])
        .assert()
        .failure();
}

#[test]
fn password_without_username_fails() {
    Command::cargo_bin("escli")
        .unwrap()
        .args(["--url", "http://localhost:9200", "--password", "bar", "info"])
        .assert()
        .failure();
}

#[test]
fn api_key_and_username_together_fails() {
    Command::cargo_bin("escli")
        .unwrap()
        .args([
            "--url",
            "http://localhost:9200",
            "--api-key",
            "key",
            "--username",
            "foo",
            "--password",
            "bar",
            "info",
        ])
        .assert()
        .failure();
}
