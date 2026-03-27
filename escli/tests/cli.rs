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
use wiremock::matchers::{body_string, header, header_exists, method, path, query_param};
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

#[tokio::test]
async fn timeout_shows_friendly_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        // Hold the response long enough that a 1-second timeout fires.
        .respond_with(
            ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)),
        )
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["--timeout", "1", "info"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("timed out"),
        "expected timeout message, got: {stderr}"
    );
}

#[tokio::test]
async fn non_utf8_response_body_shows_friendly_message() {
    let server = MockServer::start().await;
    // 0xFF 0xFE is a valid UTF-16 BOM but invalid UTF-8 — reqwest will fail
    // to decode the body when the Content-Type declares charset=utf-8.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json; charset=utf-8")
                .set_body_bytes(vec![0xFF, 0xFE, 0x00]),
        )
        .mount(&server)
        .await;

    let output = escli(&server).arg("info").output().unwrap();

    // If the client decodes lossy (no error), the garbled body goes to stdout
    // and we exit 0 — that's also acceptable. What must NOT happen is a
    // Debug-formatted panic or empty stderr with exit 1.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.is_empty(),
            "stderr must not be empty on decode error"
        );
    }
}

// --- binary response passthrough ---------------------------------------------

/// Arrow IPC bytes contain 0xFF which is invalid UTF-8.  If the response goes
/// through a text layer the byte gets replaced with the UTF-8 replacement
/// sequence (EF BF BD), corrupting the stream.  This test verifies that raw
/// bytes reach stdout untouched.
#[tokio::test]
async fn binary_response_bytes_are_not_utf8_encoded() {
    // Minimal fake Arrow IPC stream: starts with 0xFF 0xFF 0xFF 0xFF
    // (continuation marker), followed by arbitrary non-UTF-8 bytes.
    let arrow_bytes: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_query"))
        .and(query_param("format", "arrow"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/vnd.apache.arrow.stream")
                .set_body_bytes(arrow_bytes.clone()),
        )
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["esql", "query", "--format", "arrow"])
        .write_stdin(r#"{"query":"FROM test"}"#)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout, arrow_bytes,
        "stdout bytes were corrupted (UTF-8 encoding applied to binary response)"
    );
}

// --- utils dump --------------------------------------------------------------

const PIT_OK: &str = r#"{"id":"test-pit-id"}"#;
const EMPTY_SEARCH: &str = r#"{"pit_id":"test-pit-id","hits":{"hits":[]}}"#;
const ONE_DOC_SEARCH: &str = r#"{"pit_id":"test-pit-id","hits":{"hits":[{"_id":"doc1","_source":{"field":"value"},"sort":[1]}]}}"#;

#[tokio::test]
async fn dump_opens_pit_and_calls_search() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .expect(1)
        .mount(&server)
        .await;

    // Dump always makes an initial search + one pagination check before breaking.
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .expect(2)
        .mount(&server)
        .await;

    escli(&server)
        .args(["utils", "dump", "my-index"])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn dump_writes_ndjson_to_stdout() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .mount(&server)
        .await;

    // Wiremock is FIFO: first-mounted mock has highest priority.
    // One-doc response fires once (initial search), then falls through to empty (pagination check).
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ONE_DOC_SEARCH))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["utils", "dump", "my-index"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#"{"index":{"_index":"my-index"}}"#), "missing action line");
    assert!(stdout.contains(r#"{"field":"value"}"#), "missing document");
}

#[tokio::test]
async fn dump_paginates_until_empty() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .mount(&server)
        .await;

    // Two pages of results (FIFO: fires first), then falls through to empty.
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ONE_DOC_SEARCH))
        .up_to_n_times(2)
        .mount(&server)
        .await;

    // Fallback: empty (stops pagination).
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["utils", "dump", "my-index"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // 2 pages × (1 action line + 1 doc line) = 4 lines
    assert_eq!(stdout.lines().count(), 4, "expected 4 NDJSON lines for 2 pages");
}

#[tokio::test]
async fn dump_output_to_file() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ONE_DOC_SEARCH))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("dump.ndjson");

    escli(&server)
        .args(["utils", "dump", "my-index", "--output", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout("");  // nothing on stdout when writing to file

    let contents = std::fs::read_to_string(&out).unwrap();
    assert!(contents.contains(r#"{"index":{"_index":"my-index"}}"#));
    assert!(contents.contains(r#"{"field":"value"}"#));
}

#[tokio::test]
async fn dump_multiple_indices_opens_pit_for_each() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/index1/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/index2/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    escli(&server)
        .args(["utils", "dump", "index1,index2"])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn dump_pit_failure_skips_index() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/bad-index/_pit"))
        .respond_with(ResponseTemplate::new(404).set_body_string(r#"{"error":"index not found"}"#))
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["utils", "dump", "bad-index"])
        .output()
        .unwrap();

    // Should exit 0 and produce no documents — the index is skipped gracefully.
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[tokio::test]
async fn dump_skip_index_name_omits_index_from_action() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ONE_DOC_SEARCH))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["utils", "dump", "my-index", "--skip-index-name"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#"{"index":{}}"#), "action line should have no _index");
    assert!(!stdout.contains("_index"), "should not contain _index at all");
}

#[tokio::test]
async fn dump_add_id_includes_id_in_action() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ONE_DOC_SEARCH))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    let output = escli(&server)
        .args(["utils", "dump", "my-index", "--add-id"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#""_id":"doc1""#), "action line should contain _id");
    assert!(stdout.contains(r#""_index":"my-index""#), "action line should still contain _index");
}

#[tokio::test]
async fn dump_query_from_file_succeeds() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/my-index/_pit"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PIT_OK))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ONE_DOC_SEARCH))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_SEARCH))
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let query_file = dir.path().join("query.json");
    std::fs::write(&query_file, r#"{"term":{"field":"value"}}"#).unwrap();

    let output = escli(&server)
        .args(["utils", "dump", "my-index", "--query", query_file.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#"{"field":"value"}"#));
}

#[tokio::test]
async fn dump_query_bad_file_exits_1() {
    let server = MockServer::start().await;

    let output = escli(&server)
        .args(["utils", "dump", "my-index", "--query", "/nonexistent/query.json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

// --- utils load --------------------------------------------------------------

const BULK_OK: &str = r#"{"errors":false,"items":[{"index":{"status":200}}]}"#;

#[tokio::test]
async fn load_json_lines_posts_to_index_bulk() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .and(header("content-type", "application/x-ndjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.json");
    std::fs::write(&file, "{\"field\":\"value\"}\n").unwrap();

    escli(&server)
        .args(["utils", "load", "--index", "my-index", file.to_str().unwrap()])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_ndjson_posts_to_bulk() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_bulk"))
        .and(header("content-type", "application/x-ndjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.ndjson");
    std::fs::write(
        &file,
        "{\"index\":{\"_index\":\"my-index\"}}\n{\"field\":\"value\"}\n",
    )
    .unwrap();

    escli(&server)
        .args(["utils", "load", file.to_str().unwrap()])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_with_pipeline_includes_query_param() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .and(query_param("pipeline", "my-pipeline"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.json");
    std::fs::write(&file, "{\"field\":\"value\"}\n").unwrap();

    escli(&server)
        .args([
            "utils", "load",
            "--index", "my-index",
            "--pipeline", "my-pipeline",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_bulk_errors_are_reported_on_stderr() {
    let server = MockServer::start().await;
    let bulk_err = r#"{"errors":true,"items":[{"index":{"status":400,"error":{"type":"mapper_exception","reason":"failed to parse"}}}]}"#;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bulk_err))
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.json");
    std::fs::write(&file, "{\"field\":\"value\"}\n").unwrap();

    let output = escli(&server)
        .args(["utils", "load", "--index", "my-index", file.to_str().unwrap()])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected exit 1 on bulk errors");
    assert!(stderr.contains("Error"), "expected error details on stderr, got: {stderr}");
}

#[tokio::test]
async fn load_ndjson_from_stdin() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_bulk"))
        .and(header("content-type", "application/x-ndjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["utils", "load"])
        .write_stdin("{\"index\":{\"_index\":\"my-index\"}}\n{\"field\":\"value\"}\n")
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_json_from_stdin_with_format_flag() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .and(header("content-type", "application/x-ndjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["utils", "load", "--format", "json", "--index", "my-index"])
        .write_stdin("{\"field\":\"value\"}\n")
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_stdin_explicit_dash() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/_bulk"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    escli(&server)
        .args(["utils", "load", "-"])
        .write_stdin("{\"index\":{\"_index\":\"my-index\"}}\n{\"field\":\"value\"}\n")
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_bulk_http_error_exits_1() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.json");
    std::fs::write(&file, "{\"field\":\"value\"}\n").unwrap();

    escli(&server)
        .args(["utils", "load", "--index", "my-index", file.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);
}

#[tokio::test]
async fn load_multiple_batches_sends_multiple_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(2)
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.json");
    std::fs::write(&file, "{\"a\":1}\n{\"a\":2}\n").unwrap();

    escli(&server)
        .args(["utils", "load", "--index", "my-index", "--size", "1", file.to_str().unwrap()])
        .assert()
        .success();

    server.verify().await;
}

#[tokio::test]
async fn load_format_override_treats_file_as_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/my-index/_bulk"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BULK_OK))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("data.txt");
    std::fs::write(&file, "{\"field\":\"value\"}\n").unwrap();

    escli(&server)
        .args(["utils", "load", "--index", "my-index", "--format", "json", file.to_str().unwrap()])
        .assert()
        .success();

    server.verify().await;
}

#[test]
fn load_file_not_found_fails() {
    Command::cargo_bin("escli")
        .unwrap()
        .args(["--url", "http://127.0.0.1:1", "utils", "load", "--index", "my-index", "/tmp/does-not-exist-escli-test.json"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn load_json_without_index_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("docs.json");
    std::fs::write(&file, "{\"field\":\"value\"}\n").unwrap();

    Command::cargo_bin("escli")
        .unwrap()
        .args(["--url", "http://127.0.0.1:1", "utils", "load", file.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);
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
