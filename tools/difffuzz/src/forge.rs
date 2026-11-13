//! The bytecode comparison with solc through Foundry's `forge equivalence`.
//!
//! Where the random calls compare the two compilers' bytecode on the inputs a case happens to
//! try, forge explores every calldata symbolically, from arbitrary storage, and reports an input
//! on which the deployed codes differ in success, return or revert data, storage, logs or
//! balances. Runs that only exhaust gas are left out (`--ignore-out-of-gas`): the compilers do not
//! promise gas parity.

use crate::compile;
use alloy_primitives::hex;
use std::{path::Path, process::Command, time::Duration};

/// What one `forge equivalence` run concluded.
#[derive(Debug)]
pub(crate) struct Equivalence {
    /// `equivalent`, `not_equivalent` or `incomplete`.
    pub(crate) status: String,
    /// The distinguishing calldata, when not equivalent.
    pub(crate) calldata: Vec<u8>,
    /// The JSON report, or the error of an incomplete run.
    pub(crate) report: String,
}

/// Checks two runtime codes for equivalence with the `forge` at `forge`.
pub(crate) fn equivalence(
    forge: &str,
    a: &[u8],
    b: &[u8],
    dir: &Path,
    timeout: Duration,
) -> Result<Equivalence, String> {
    let fa = dir.join("forge-a.hex");
    let fb = dir.join("forge-b.hex");
    std::fs::write(&fa, hex::encode(a)).map_err(|e| e.to_string())?;
    std::fs::write(&fb, hex::encode(b)).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(forge);
    cmd.arg("equivalence")
        .arg("--code-a-file")
        .arg(&fa)
        .arg("--code-b-file")
        .arg(&fb)
        .arg("--ignore-out-of-gas")
        .arg("--timeout")
        .arg(timeout.as_secs().to_string())
        .arg("--json")
        .env("FOUNDRY_DISABLE_NIGHTLY_WARNING", "1");
    let (_, out, err) = compile::run_status(cmd, timeout + Duration::from_secs(30))?;
    Ok(parse(&String::from_utf8_lossy(&out), &String::from_utf8_lossy(&err)))
}

/// Reads the last JSON line of a run; a run without one stopped early and says why on stderr.
pub(crate) fn parse(stdout: &str, stderr: &str) -> Equivalence {
    let json = stdout.lines().rev().find(|l| l.starts_with('{'));
    let Some(v) = json.and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok()) else {
        let error = stderr.lines().find(|l| l.contains("Error")).unwrap_or(stderr.trim());
        return Equivalence { status: "incomplete".into(), calldata: vec![], report: error.into() };
    };
    let calldata = v["calldata"]
        .as_str()
        .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
        .unwrap_or_default();
    Equivalence {
        status: v["status"].as_str().unwrap_or("incomplete").into(),
        calldata,
        report: json.unwrap_or_default().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reports() {
        let e = parse(r#"{"status":"equivalent","stats":{}}"#, "");
        assert_eq!(e.status, "equivalent");
        let e = parse(r#"{"status":"not_equivalent","calldata":"0x01ff","args":[]}"#, "");
        assert_eq!((e.status.as_str(), e.calldata), ("not_equivalent", vec![1, 0xff]));
        let e = parse("", "Error: equivalence check incomplete (Stuck): symbolic BLOCKHASH\n");
        assert_eq!(e.status, "incomplete");
        assert!(e.report.contains("BLOCKHASH"));
    }
}
