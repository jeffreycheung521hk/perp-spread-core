//! End-to-end byte/framing regressions for review probes P7a/P7b and P5.
use std::io::Write;
use std::process::{Command, Stdio};

fn valid() -> &'static str {
    "v1\thyperliquid\tauto\t1000\t0.001\t4.5\t5\t2\t2\t5\t1000\t1000\t1000\t99.99@100\t100.01@100\t100.39@100\t100.41@100"
}
fn run(input: &[u8]) -> Vec<Vec<String>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_spread-core"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect()
}

#[test]
fn p7a_blank_line_gets_an_error_response_between_valid_requests() {
    let rows = run(format!("{}\n\n{}\n", valid(), valid()).as_bytes());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][1], "OK");
    assert_eq!(rows[1], vec!["unknown", "ERR", "empty request"]);
    assert_eq!(rows[2][1], "OK");
}
#[test]
fn p7b_invalid_utf8_gets_one_error_and_does_not_stop_the_process() {
    let mut input = format!("{}\n", valid()).into_bytes();
    input.extend_from_slice(b"bad\xff\n");
    input.extend_from_slice(format!("{}\n", valid()).as_bytes());
    let rows = run(&input);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][1], "OK");
    assert_eq!(rows[1], vec!["unknown", "ERR", "invalid UTF-8 input"]);
    assert_eq!(rows[2][1], "OK");
}
#[test]
fn crlf_blank_and_final_unterminated_line_each_get_one_response() {
    let rows = run(format!("{}\r\n\r\n{}", valid(), valid()).as_bytes());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][1], "OK");
    assert_eq!(rows[1][1], "ERR");
    assert_eq!(rows[2][1], "OK");
}
#[test]
fn malformed_request_and_blank_first_line_do_not_poison_later_requests() {
    let rows = run(format!("\nmalformed\trequest\n{}\n", valid()).as_bytes());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][1], "ERR");
    assert_eq!(rows[1][1], "ERR");
    assert_eq!(rows[2][1], "OK");
}
#[test]
fn p5_fixed_quantity_ignores_unused_budget_but_auto_still_validates_it() {
    let fixed = valid().replacen("\tauto\t1000\t", "\t10\t0\t", 1);
    let invalid_auto = valid().replacen("\tauto\t1000\t", "\tauto\t0\t", 1);
    let rows = run(format!("{fixed}\n{invalid_auto}\n{}\n", valid()).as_bytes());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][1], "OK");
    assert_eq!(rows[0][2], "10.000000");
    assert_eq!(rows[1][1], "ERR");
    assert_eq!(rows[2][1], "OK");
}
#[test]
fn empty_stream_contains_no_records() {
    assert!(run(b"").is_empty());
}
