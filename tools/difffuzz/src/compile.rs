//! Compiles a source with Solar, keeping its MIR and EVM IR dumps, and with solc.

use alloy_json_abi::JsonAbi;
use alloy_primitives::hex;
use serde_json::Value;
use std::{collections::HashMap, path::Path, process::Command, time::Duration};

/// One compiled contract.
#[derive(Clone, Debug, Default)]
pub(crate) struct Artifact {
    pub(crate) abi: JsonAbi,
    pub(crate) bin: Vec<u8>,
    pub(crate) runtime: Vec<u8>,
    /// The final lowered MIR module (`-Zdump=mir-final`).
    pub(crate) mir: Option<String>,
    /// The scheduled runtime EVM IR, before the EVM IR pipeline.
    pub(crate) evmir_scheduled: Option<String>,
    /// The runtime EVM IR as assembled, after the pipeline.
    pub(crate) evmir_final: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Tools {
    pub(crate) solar: String,
    pub(crate) solar_args: Vec<String>,
    pub(crate) solc: String,
    pub(crate) solc_args: Vec<String>,
    pub(crate) timeout: Duration,
}

/// Runs a command with a timeout; returns stdout or an error with stderr.
pub(crate) fn run(cmd: Command, timeout: Duration) -> Result<Vec<u8>, String> {
    let (ok, out, err) = run_status(cmd, timeout)?;
    if !ok {
        let mut msg = String::from_utf8_lossy(&err).into_owned();
        if msg.trim().is_empty() {
            msg = String::from_utf8_lossy(&out).into_owned();
        }
        return Err(format!("failed: {}", msg.trim()));
    }
    Ok(out)
}

/// Runs a command with a timeout; returns whether it succeeded, its stdout and its stderr.
pub(crate) fn run_status(
    mut cmd: Command,
    timeout: Duration,
) -> Result<(bool, Vec<u8>, Vec<u8>), String> {
    use std::io::Read;
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {cmd:?}: {e}"))?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut v = vec![];
        let _ = stdout.read_to_end(&mut v);
        v
    });
    let err = std::thread::spawn(move || {
        let mut v = vec![];
        let _ = stderr.read_to_end(&mut v);
        v
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timeout after {timeout:?}"));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let out = out.join().unwrap();
    let err = err.join().unwrap();
    Ok((status.success(), out, err))
}

fn json_in(output: &[u8]) -> Result<Value, String> {
    if let Ok(v) = serde_json::from_slice::<Value>(output) {
        return Ok(v);
    }
    (0..output.len())
        .rev()
        .filter(|&s| output[s] == b'{' && (s == 0 || output[s - 1] == b'\n'))
        .find_map(|s| {
            let v = serde_json::Deserializer::from_slice(&output[s..])
                .into_iter::<Value>()
                .next()?
                .ok()?;
            v.get("contracts").is_some().then_some(v)
        })
        .ok_or_else(|| "no compiler JSON in the output".into())
}

fn artifacts(json: &Value) -> Result<HashMap<String, Artifact>, String> {
    let contracts = json.get("contracts").and_then(Value::as_object).ok_or("no contracts")?;
    let mut out = HashMap::new();
    for (name, c) in contracts {
        let short = name.rsplit(':').next().unwrap().to_string();
        let abi = match c.get("abi") {
            Some(Value::String(s)) => serde_json::from_str(s).map_err(|e| e.to_string())?,
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| e.to_string())?,
            None => JsonAbi::new(),
        };
        let hexs = |k: &str| {
            c.get(k)
                .and_then(Value::as_str)
                .map(|s| hex::decode(s).unwrap_or_default())
                .unwrap_or_default()
        };
        out.insert(
            short,
            Artifact { abi, bin: hexs("bin"), runtime: hexs("bin-runtime"), ..Artifact::default() },
        );
    }
    Ok(out)
}

impl Tools {
    fn solar_cmd(&self, file: &Path, dump: &str) -> Command {
        let mut cmd = Command::new(&self.solar);
        cmd.args(&self.solar_args)
            .arg("--emit=abi,bin,bin-runtime")
            .arg(format!("-Zdump={dump}"))
            .arg("--color=never")
            .arg(file);
        cmd
    }

    /// Compiles `file` with Solar three times, for the MIR and both EVM IR dumps.
    pub(crate) fn solar(&self, file: &Path, contract: &str) -> Result<Artifact, String> {
        let out = run(self.solar_cmd(file, "mir-final"), self.timeout)?;
        let mut art = artifacts(&json_in(&out)?)?.remove(contract).ok_or("no such contract")?;
        let text = String::from_utf8_lossy(&out);
        art.mir = solar_mir_interpreter::parse_dump(&text)
            .into_iter()
            .find(|m| m.name == contract || m.name.ends_with(&format!(":{contract}")))
            .map(|m| m.mir);
        for (dump, program) in
            [("evm-ir-runtime-scheduled", "scheduled runtime"), ("evm-ir-runtime", "runtime")]
        {
            let out = run(self.solar_cmd(file, dump), self.timeout)?;
            let again = artifacts(&json_in(&out)?)?.remove(contract).ok_or("no such contract")?;
            if again.runtime != art.runtime {
                return Err(format!("the {dump} dump changed the runtime bytecode"));
            }
            let text = String::from_utf8_lossy(&out);
            let module = solar_evmir_interpreter::parse_dump(&text)
                .into_iter()
                .filter(|m| m.program.as_deref() == Some(program))
                .find(|m| m.name == contract || m.name.ends_with(&format!(":{contract}")))
                .map(|m| m.evm_ir);
            if program == "runtime" {
                art.evmir_final = module;
            } else {
                art.evmir_scheduled = module;
            }
        }
        Ok(art)
    }

    /// Compiles `file` with solc.
    pub(crate) fn solc(&self, file: &Path, contract: &str) -> Result<Artifact, String> {
        let mut cmd = Command::new(&self.solc);
        cmd.args(&self.solc_args).args(["--combined-json", "abi,bin,bin-runtime"]).arg(file);
        let out = run(cmd, self.timeout)?;
        artifacts(&json_in(&out)?)?.remove(contract).ok_or_else(|| "no such contract".into())
    }
}
