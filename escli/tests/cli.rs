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
use wiremock::matchers::{header_exists, method, path};
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
