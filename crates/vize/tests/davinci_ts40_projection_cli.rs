//! Exact TS-40 presentation baseline for `vize check --show-virtual-ts`.
//!
//! The socket intentionally supplies a deterministic projection oracle. Canon,
//! Content Mapper, and Maestro generation are frozen independently by the
//! companion `vize_maestro` integration test.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::io::{BufRead, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use vize_s0::{String, ToCompactString, append, cstr};

const SOCKET_ORACLE_TIMEOUT: Duration = Duration::from_secs(30);
const SOCKET_ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const NO_CONNECT_TEST_TIMEOUT: Duration = Duration::from_millis(25);

#[test]
#[allow(clippy::disallowed_macros)] // `insta` expands to `format!`.
fn show_virtual_ts_presents_the_exact_fixture_matrix() {
    let root = workspace_root();
    let matrix: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("tests/_fixtures/davinci-ts40-projection/matrix.json"))
            .unwrap(),
    )
    .unwrap();
    let fixtures = matrix["fixtures"].as_array().unwrap();
    assert!(!fixtures.is_empty(), "TS-40 CLI matrix must not be vacuous");

    let project = root
        .join("target/vize-tests/tests")
        .join(cstr!("davinci-ts40-cli-{}", std::process::id()).as_str());
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    let project = project.canonicalize().unwrap();

    let mut expected = BTreeMap::new();
    for fixture in fixtures {
        let id = fixture["id"].as_str().unwrap();
        let source_path = root.join(fixture["file"].as_str().unwrap());
        let mut source = std::fs::read_to_string(source_path).unwrap();
        if fixture["lineEnding"] == "crlf" {
            source = source.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        let name = cstr!("{id}.vue");
        std::fs::write(project.join("src").join(name.as_str()), &source).unwrap();
        expected.insert(name, digest(&source));
    }

    let socket_name = cstr!("/tmp/vize-ts40-cli-{}.sock", std::process::id());
    let socket = PathBuf::from(socket_name.as_str());
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).unwrap();
    let expected_count = expected.len();
    let server = std::thread::spawn(move || {
        let mut stream = accept_with_deadline(&listener, SOCKET_ORACLE_TIMEOUT).unwrap();
        stream
            .set_read_timeout(Some(SOCKET_ORACLE_TIMEOUT))
            .unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        for _ in 0..expected_count {
            #[allow(clippy::disallowed_types)]
            let mut line = std::string::String::new();
            reader.read_line(&mut line).unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "check");
            let uri = request["params"]["uri"].as_str().unwrap();
            let name = Path::new(uri).file_name().unwrap().to_str().unwrap();
            let source = request["params"]["content"].as_str().unwrap();
            let source_digest = digest(source);
            assert_eq!(expected.get(name), Some(&source_digest));

            let diagnostics = if name == "parse-recovery.vue" {
                serde_json::json!([{
                    "message": "TS-40 recovery sentinel",
                    "severity": "warning",
                    "line": 1,
                    "column": 1,
                    "code": "TS40"
                }])
            } else {
                serde_json::json!([])
            };
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": {
                    "diagnostics": diagnostics,
                    "virtualTs": cstr!("// TS-40 {name} {source_digest}"),
                    "errorCount": 0
                }
            });
            writeln!(stream, "{response}").unwrap();
        }
    });

    let output = Command::new(env!("CARGO_BIN_EXE_vize"))
        .current_dir(&project)
        .args([
            "check",
            "src",
            "--socket",
            socket.to_str().unwrap(),
            "--format",
            "json",
            "--show-virtual-ts",
        ])
        .output()
        .unwrap();
    let server_result = server.join();
    let _ = std::fs::remove_file(&socket);

    assert!(
        output.status.success(),
        "check failed: {}",
        std::str::from_utf8(&output.stderr).unwrap_or("<non-UTF-8 stderr>")
    );
    server_result.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["fileCount"], fixtures.len());
    assert_eq!(json["warningCount"], 1);
    insta::assert_snapshot!(
        "davinci_ts40_show_virtual_ts_matrix",
        serde_json::to_string_pretty(&json).unwrap()
    );

    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    let mut expected_stderr = cstr!(
        "Connected to check-server at {}\nType checking {} Vue files...\n\n=== {} ===\n{}\n",
        socket.display(),
        fixtures.len(),
        vize_canon::virtual_ts::SHARED_PREAMBLE_FILE_NAME,
        vize_canon::virtual_ts::SHARED_PREAMBLE_DTS
    );
    for file in json["files"].as_array().unwrap() {
        let path = project.join(file["file"].as_str().unwrap());
        let virtual_ts = file["virtualTs"].as_str().unwrap();
        append!(
            expected_stderr,
            "\n=== {} ===\n{virtual_ts}\n",
            path.display()
        );
    }
    assert_eq!(stderr, expected_stderr);
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn socket_oracle_times_out_when_the_cli_exits_before_connecting() {
    let socket_name = cstr!("/tmp/vize-ts40-cli-no-connect-{}.sock", std::process::id());
    let socket = PathBuf::from(socket_name.as_str());
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_vize"))
        .args([
            "check",
            "--socket",
            socket.to_str().unwrap(),
            "--davinci-no-connect-probe",
        ])
        .output()
        .unwrap();
    let accept_error = accept_with_deadline(&listener, NO_CONNECT_TEST_TIMEOUT).unwrap_err();
    let _ = std::fs::remove_file(&socket);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        std::str::from_utf8(&output.stderr).unwrap(),
        "error: unexpected argument '--davinci-no-connect-probe' found\n\n  tip: to pass '--davinci-no-connect-probe' as a value, use '-- --davinci-no-connect-probe'\n\nUsage: vize check --socket <SOCKET> [PATTERNS]...\n\nFor more information, try '--help'.\n"
    );
    assert_eq!(accept_error.kind(), ErrorKind::TimedOut);
    assert_eq!(
        accept_error.to_compact_string(),
        "vize check did not connect to the TS-40 oracle socket before its deadline"
    );
}

fn accept_with_deadline(listener: &UnixListener, timeout: Duration) -> std::io::Result<UnixStream> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "vize check did not connect to the TS-40 oracle socket before its deadline",
                    ));
                }
                std::thread::sleep(remaining.min(SOCKET_ACCEPT_POLL_INTERVAL));
            }
            Err(error) => return Err(error),
        }
    }
}

fn digest(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    use std::fmt::Write as _;
    for byte in digest {
        write!(out, "{byte:02x}").unwrap();
    }
    out
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
