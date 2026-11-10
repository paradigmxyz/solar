//! The edge coverage plugin: AFL++-style probes on a CFG with its critical edges split, and
//! comparison operand logs before every `eq`, `lt`, `gt`, `slt` and `sgt`.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

const SOURCE: &str = "\
contract Sample {
    uint256 public total;

    function step(uint256 x) public returns (uint256) {
        uint256 acc;
        for (uint256 i = 0; i < x; i++) {
            if (i % 3 == 0) {
                acc += helper(i);
            } else if (i % 3 == 1) {
                acc -= 1;
            } else {
                acc *= 2;
            }
        }
        total += acc;
        return acc;
    }

    function helper(uint256 y) internal pure returns (uint256) {
        return y > 10 ? y - 10 : y + 1;
    }
}
";

const EDGECOV: u8 = 0xc0;
const CMPLOG: u8 = 0xc1;
/// The logged comparisons: `lt`, `gt`, `slt`, `sgt` and `eq`.
const COMPARISONS: [(u8, &str); 5] =
    [(0x10, "lt"), (0x11, "gt"), (0x12, "slt"), (0x13, "sgt"), (0x14, "eq")];

fn edgecov() -> PathBuf {
    library_path("solar_edgecov_plugin")
}

/// One printed EVM IR block: its label and instruction lines, the terminator last.
struct Block {
    label: String,
    lines: Vec<String>,
}

/// Splits the modules printed after `edgecov::instrument` into `(header, blocks)`.
fn modules(text: &str) -> Vec<(String, Vec<Block>)> {
    let mut modules = Vec::<(String, Vec<Block>)>::new();
    let mut current = false;
    for line in text.lines() {
        if line.starts_with("; ===") {
            current = line.ends_with("(after edgecov::instrument) ===");
            if current {
                modules.push((line.to_owned(), Vec::new()));
            }
        } else if !current {
            continue;
        } else if let Some(rest) = line.strip_prefix("bb") {
            let label = format!("bb{}", rest.split([':', ' ']).next().unwrap());
            modules.last_mut().unwrap().1.push(Block { label, lines: Vec::new() });
        } else if let Some(inst) = line.strip_prefix("  ")
            && let Some((_, blocks)) = modules.last_mut()
            && let Some(block) = blocks.last_mut()
        {
            block.lines.push(inst.split(" ;").next().unwrap().trim().to_owned());
        }
    }
    modules
}

/// The ID word of a glued `push <word>; <opcode>` probe at `at`.
fn glued_word(lines: &[String], at: usize, opcode: &str) -> Option<u64> {
    let word = lines.get(at)?.strip_prefix("push ")?.strip_suffix(" !metadata(keep_with_next)")?;
    (lines.get(at + 1)? == opcode).then(|| match word.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).unwrap(),
        None => word.parse().unwrap(),
    })
}

fn probe_id(lines: &[String], at: usize) -> Option<u32> {
    glued_word(lines, at, "edgecov").map(|id| u32::try_from(id).unwrap())
}

fn pushed_block(line: &str) -> Option<&str> {
    line.strip_prefix("push ").filter(|target| target.starts_with("bb"))
}

fn terminator_targets(line: &str) -> Vec<&str> {
    ["jump ", "jumpi ", "indexed_jump "]
        .into_iter()
        .find_map(|prefix| line.strip_prefix(prefix))
        .map(|targets| targets.split(", ").collect())
        .unwrap_or_default()
}

/// Checks the probe placement of one instrumented module and returns every edge probe ID and
/// comparison site ID. Every `jumpi` continuation starts with a probe, and no critical edge
/// remains. In `blocks` mode every block starts with a probe; in `branches` mode the entry and
/// every single-predecessor target of a conditional edge do. With `cmplog`, every comparison
/// directly follows a glued `push <(site << 8) | opcode>; cmplog`; without it, none does.
fn check_instrumented(blocks: &[Block], mode: &str, cmplog: bool) -> (Vec<u32>, Vec<u32>) {
    let mut predecessors = BTreeMap::<&str, u32>::new();
    let mut dynamic = BTreeSet::new();
    *predecessors.entry(&blocks[0].label).or_default() += 1;
    for block in blocks {
        let (terminator, body) = block.lines.split_last().unwrap();
        for target in terminator_targets(terminator) {
            *predecessors.entry(target).or_default() += 1;
        }
        for (index, line) in body.iter().enumerate() {
            let Some(target) = pushed_block(line) else { continue };
            if body.get(index + 1).is_some_and(|next| next == "jumpi") {
                *predecessors.entry(target).or_default() += 1;
            } else {
                dynamic.insert(target);
            }
        }
    }
    let unique = |target: &str| predecessors[target] == 1 && !dynamic.contains(target);
    let mut must_start_probed = BTreeSet::from([blocks[0].label.as_str()]);
    for block in blocks {
        let label = &block.label;
        let (terminator, body) = block.lines.split_last().unwrap();
        if mode == "blocks" {
            must_start_probed.insert(label);
        }
        for (index, line) in body.iter().enumerate() {
            if line != "jumpi" {
                continue;
            }
            if let Some(target) = pushed_block(&body[index - 1]) {
                assert!(unique(target), "critical edge {label} -> {target}");
                must_start_probed.insert(target);
            }
            if index + 1 < body.len() {
                assert!(
                    probe_id(body, index + 1).is_some(),
                    "{label} continues unprobed after jumpi"
                );
            }
        }
        let targets = terminator_targets(terminator);
        if targets.len() + usize::from(body.last().is_some_and(|last| last == "jumpi")) > 1 {
            for target in targets {
                assert!(unique(target), "critical edge {label} -> {target}");
                must_start_probed.insert(target);
            }
        }
    }
    let (mut ids, mut cmp_ids) = (Vec::new(), Vec::new());
    for block in blocks {
        let label = block.label.as_str();
        let body = &block.lines[..block.lines.len() - 1];
        if must_start_probed.contains(label) {
            assert!(probe_id(body, 0).is_some(), "{label} starts without a probe");
        }
        ids.extend((0..body.len()).filter_map(|at| probe_id(body, at)));
        let logged_before = cmp_ids.len();
        for (at, line) in body.iter().enumerate() {
            let Some(&(opcode, _)) = COMPARISONS.iter().find(|(_, name)| name == line) else {
                continue;
            };
            let word = at.checked_sub(2).and_then(|push| glued_word(body, push, "cmplog"));
            if !cmplog {
                assert!(word.is_none(), "{label}: {line} is logged with cmplog disabled");
                continue;
            }
            let word = word.unwrap_or_else(|| panic!("{label}: {line} at {at} is not logged"));
            assert_eq!(word & 0xff, u64::from(opcode), "{label}: {line} logged as another kind");
            cmp_ids.push(u32::try_from(word >> 8).unwrap());
        }
        let logs = body.iter().filter(|line| *line == "cmplog").count();
        assert_eq!(logs, cmp_ids.len() - logged_before, "{label}: cmplog without a comparison");
    }
    (ids, cmp_ids)
}

/// Every `0xc0` opcode of the bytecode, as the edge ID its push gives.
fn bytecode_probes(code: &str) -> Vec<u32> {
    bytecode_sites(code).0
}

/// The edge IDs of every `0xc0` opcode and the words of every `0xc1` opcode of the bytecode, as
/// their pushes give them. Checks that every comparison opcode directly follows a `cmplog` that
/// names it, when the bytecode logs comparisons at all.
fn bytecode_sites(code: &str) -> (Vec<u32>, Vec<u64>) {
    let mut code = alloy_primitives::hex::decode(code).unwrap();
    // The CBOR metadata trailer is data, not code: its last two bytes give its length.
    if let [.., high, low] = code[..]
        && let Some(start) =
            code.len().checked_sub(2 + usize::from(u16::from_be_bytes([high, low])))
        && (0xa1..=0xa8).contains(&code[start])
    {
        code.truncate(start);
    }
    let (mut edges, mut cmps, mut previous, mut index) = (Vec::new(), Vec::new(), None::<u64>, 0);
    // The comparisons, each with the word of the `cmplog` directly before it, if any.
    let mut comparisons = Vec::new();
    let mut logged = None;
    while index < code.len() {
        let opcode = code[index];
        if opcode == EDGECOV {
            let id = previous.take().filter(|&id| id != 0).and_then(|id| u32::try_from(id).ok());
            edges.push(id.expect("edgecov must follow a nonzero push of at most 4 bytes"));
            logged = None;
            index += 1;
            continue;
        }
        if opcode == CMPLOG {
            let word = previous.take().filter(|&word| word >> 8 != 0);
            let word = word.expect("cmplog must follow a push of a nonzero site");
            cmps.push(word);
            logged = Some(word);
            index += 1;
            continue;
        }
        if COMPARISONS.iter().any(|&(comparison, _)| comparison == opcode) {
            comparisons.push((opcode, logged));
        }
        previous = None;
        logged = None;
        if (0x60..=0x7f).contains(&opcode) {
            let width = usize::from(opcode - 0x5f);
            let immediate = &code[index + 1..(index + 1 + width).min(code.len())];
            if width <= 8 {
                previous =
                    Some(immediate.iter().fold(0, |word, &byte| word << 8 | u64::from(byte)));
            }
            index += width;
        }
        index += 1;
    }
    if !cmps.is_empty() {
        for (opcode, word) in comparisons {
            assert_eq!(
                word.map(|word| word & 0xff),
                Some(u64::from(opcode)),
                "unlogged {opcode:#x}"
            );
        }
    }
    (edges, cmps)
}

fn contract_output(artifact: &Value, output: &str) -> String {
    let contract = artifact["contracts"].as_object().unwrap().values().next().unwrap();
    contract[output].as_str().unwrap().to_owned()
}

/// Instruments the sample in `mode`, with comparison operand logging when `cmplog` is set, checks
/// every module and that the probes leave its behaviour unchanged, and returns a summary of the
/// probe counts.
fn instrument_sample(mode: &str, cmplog: bool) -> String {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("Sample.sol");
    fs::write(&source, SOURCE).unwrap();
    let plain = Command::new(SOLAR)
        .args(["--allow=2264", "-j1", "-Ogas", "--emit=bin", "-Zevm-ir-pipeline=none"])
        .arg("-Zdump=evm-ir-runtime")
        .arg(&source)
        .output()
        .unwrap();
    assert!(plain.status.success(), "{}", String::from_utf8_lossy(&plain.stderr));
    let plain = String::from_utf8(plain.stdout).unwrap();
    let instrumented = Command::new(SOLAR)
        .args(["--allow=2264", "-j1", "--error-format=rustc-json"])
        .arg(format!("-Zload-pass-plugin={}", edgecov().display()))
        .args([
            "-Ogas",
            "--emit=bin",
            "-Zevm-ir-pipeline=edgecov::instrument",
            "-Zprint-after-each",
        ])
        .arg(&source)
        .env("SOLAR_EDGECOV_MODE", mode)
        .env("SOLAR_EDGECOV_CMPLOG", if cmplog { "1" } else { "0" })
        .output()
        .unwrap();
    assert!(instrumented.status.success(), "{}", String::from_utf8_lossy(&instrumented.stderr));
    let instrumented = String::from_utf8(instrumented.stdout).unwrap();
    let first = instrumented.find("(after edgecov::instrument)").unwrap();
    let instrumented = &instrumented[instrumented[..first].rfind("; ===").unwrap()..];
    let runtime = |text: &str| {
        let start = text.find("@module Sample_runtime").unwrap();
        let lines = text[start..].lines().take_while(|line| !line.starts_with(['{', ';']));
        lines.map(|line| format!("{line}\n")).collect::<String>()
    };
    let (plain, instrumented_runtime) = (runtime(&plain), runtime(instrumented));
    let plain_blocks = plain.lines().filter(|line| line.starts_with("bb")).count();

    let mut all_ids = Vec::new();
    let mut summary = format!("uninstrumented runtime blocks={plain_blocks}\n");
    for (header, blocks) in modules(instrumented) {
        let (ids, cmp_ids) = check_instrumented(&blocks, mode, cmplog);
        let module_ids = ids.iter().chain(&cmp_ids);
        let (min, max) = (module_ids.clone().min().unwrap(), module_ids.max().unwrap());
        summary.push_str(&format!(
            "{header}\nblocks={} probes={} cmps={} ids={min}..={max}\n",
            blocks.len(),
            ids.len(),
            cmp_ids.len()
        ));
        all_ids.extend(ids.into_iter().chain(cmp_ids));
    }
    // Edge and comparison site IDs are unique, disjoint and dense from 1 across every module of
    // the process.
    all_ids.sort_unstable();
    assert_eq!(all_ids, (1..=all_ids.len() as u32).collect::<Vec<_>>());

    // The probes do not change what the module computes.
    for x in [0_u64, 1, 2, 3, 7, 20] {
        let mut calldata = vec![0x3a, 0x1c, 0xde, 0x75];
        calldata.extend(alloy_primitives::U256::from(x).to_be_bytes::<32>());
        let expected = solar_evmir_interpreter::transact(
            &plain,
            &calldata,
            &mut EmptyHost,
            Default::default(),
        )
        .unwrap();
        let actual = solar_evmir_interpreter::transact(
            &instrumented_runtime,
            &calldata,
            &mut EmptyHost,
            Default::default(),
        )
        .unwrap();
        assert_eq!(actual.outcome, expected.outcome, "{mode} step({x})");
    }
    summary
}

#[test]
fn edge_coverage_probes_blocks_and_splits_critical_edges() {
    assert_data_eq!(
        instrument_sample("blocks", true),
        str![[r#"
uninstrumented runtime blocks=19
; === Sample_runtime (after edgecov::instrument) ===
blocks=28 probes=29 cmps=8 ids=1..=37
; === Sample_deployment (after edgecov::instrument) ===
blocks=2 probes=3 cmps=0 ids=38..=40

"#]]
    );
}

#[test]
fn edge_coverage_probes_only_conditional_edges_by_default() {
    assert_data_eq!(
        instrument_sample("branches", true),
        str![[r#"
uninstrumented runtime blocks=19
; === Sample_runtime (after edgecov::instrument) ===
blocks=28 probes=23 cmps=8 ids=1..=31
; === Sample_deployment (after edgecov::instrument) ===
blocks=2 probes=3 cmps=0 ids=32..=34

"#]]
    );
}

#[test]
fn edge_coverage_survives_optimization_as_glued_probes() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("Sample.sol");
    fs::write(&source, SOURCE).unwrap();
    for mode in ["none", "gas", "size"] {
        let baseline = Command::new(SOLAR)
            .args(["--allow=2264", "-j1", "-O", mode, "--emit=bin,bin-runtime"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(baseline.status.success(), "{}", String::from_utf8_lossy(&baseline.stderr));
        let instrumented = run(&source, &edgecov(), &["-O", mode, "--emit=bin,bin-runtime"]);
        assert!(instrumented.status.success(), "{}", String::from_utf8_lossy(&instrumented.stderr));
        let (baseline, instrumented) = (artifact(&baseline), artifact(&instrumented));
        for output in ["bin", "bin-runtime"] {
            let plain = contract_output(&baseline, output);
            assert!(bytecode_probes(&plain).is_empty(), "{mode} {output}");
        }
        // Every probe keeps its push, every comparison stays logged, and each edge and comparison
        // site ID stays distinct.
        let (edges, cmps) = bytecode_sites(&contract_output(&instrumented, "bin-runtime"));
        let mut ids = edges.iter().map(|&id| u64::from(id)).collect::<Vec<_>>();
        ids.extend(cmps.iter().map(|word| word >> 8));
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert!(
            edges.len() > 10 && cmps.len() > 3,
            "{mode}: {} edges, {} cmps",
            edges.len(),
            cmps.len()
        );
        assert!(ids.len() == count, "{mode}: {count} probes, {} IDs", ids.len());
    }
}

#[test]
fn edge_coverage_without_cmplog() {
    assert_data_eq!(
        instrument_sample("branches", false),
        str![[r#"
uninstrumented runtime blocks=19
; === Sample_runtime (after edgecov::instrument) ===
blocks=28 probes=23 cmps=0 ids=1..=23
; === Sample_deployment (after edgecov::instrument) ===
blocks=2 probes=3 cmps=0 ids=24..=26

"#]]
    );
}

#[test]
fn cmplog_sites_are_mapped() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("Sample.sol");
    let map = directory.path().join("edges.map");
    fs::write(&source, SOURCE).unwrap();
    let output = Command::new(SOLAR)
        .args(["--allow=2264", "-j1", "-Ogas", "--emit=bin-runtime"])
        .arg(format!("-Zload-pass-plugin={}", edgecov().display()))
        .arg(&source)
        .env("SOLAR_EDGECOV_MAP", &map)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let (edges, cmps) = bytecode_sites(&contract_output(&artifact(&output), "bin-runtime"));
    // <id> <module> <block label> <kind>
    #[allow(clippy::disallowed_methods)]
    let map = fs::read_to_string(&map).unwrap();
    let kinds = map
        .lines()
        .map(|line| {
            let fields = line.split(' ').collect::<Vec<_>>();
            (fields[0].parse::<u64>().unwrap(), fields[3].to_owned())
        })
        .collect::<BTreeMap<_, _>>();
    for id in edges {
        assert!(!kinds[&u64::from(id)].starts_with("cmp:"), "edge {id}: {}", kinds[&u64::from(id)]);
    }
    for word in cmps {
        let (_, name) =
            COMPARISONS.iter().find(|&&(opcode, _)| u64::from(opcode) == word & 0xff).unwrap();
        let kind = &kinds[&(word >> 8)];
        assert!(kind.starts_with(&format!("cmp:{name}:")), "site {}: {kind}", word >> 8);
    }
}

#[test]
fn edge_coverage_instruments_standard_json() {
    let input = json!({
        "language": "Solidity",
        "sources": { "Sample.sol": { "content": SOURCE } },
        "settings": {
            "outputSelection": { "*": { "*": ["evm.deployedBytecode.object", "metadata"] } }
        }
    });
    let mut child = Command::new(SOLAR)
        .arg(format!("-Zload-pass-plugin={}", edgecov().display()))
        .arg("--standard-json")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(child.stdin.take().unwrap(), &input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output: Value = serde_json::from_slice(&output.stdout).unwrap();
    let contract = &output["contracts"]["Sample.sol"]["Sample"];
    let code = contract["evm"]["deployedBytecode"]["object"].as_str().unwrap();
    assert!(!bytecode_probes(code).is_empty());
    let metadata: Value = serde_json::from_str(contract["metadata"].as_str().unwrap()).unwrap();
    let plugins = &metadata["settings"]["solar"]["plugins"];
    assert!(plugins[0].as_str().is_some_and(|plugin| plugin.starts_with("edgecov@")), "{plugins}");
}
