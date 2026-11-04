use super::*;

/// MIR input pipelines print their text before the combined JSON artifact.
pub(super) fn artifact(output: &Output) -> Value {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("contracts").is_some())
        .expect("compiler artifact JSON")
}
