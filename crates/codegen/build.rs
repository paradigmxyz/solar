//! Compiles the ISLE rewrite rules under `isle/` into Rust source.

use std::{env, fs, path::PathBuf};

/// Rule sets and the ISLE files each one is compiled from, prelude first.
const RULE_SETS: &[(&str, &[&str])] = &[
    ("select", &["mir/prelude.isle", "evm-ir/prelude.isle", "mir-to-evm/select.isle"]),
    ("stack_select", &["mir/prelude.isle", "mir/extractors.isle", "mir-to-evm/stack_select.isle"]),
    ("egraph", &["mir/prelude.isle", "mir/extractors.isle", "mir/egraph.isle", "mir/word.isle"]),
    (
        "word_sequence",
        &[
            "mir/prelude.isle",
            "mir/extractors.isle",
            "mir/word_sequence_prelude.isle",
            "mir/word_sequence.isle",
        ],
    ),
    (
        "peephole",
        &[
            "evm-ir/prelude.isle",
            "evm-ir/peephole.isle",
            "evm-ir/stack_peephole.isle",
            "evm-ir/late_word.isle",
        ],
    ),
];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let isle_dir = manifest_dir.join("isle");
    // Keep the file names embedded in generated code independent of the checkout location.
    let options = cranelift_isle::codegen::CodegenOptions {
        exclude_global_allow_pragmas: true,
        prefixes: vec![cranelift_isle::codegen::Prefix {
            prefix: manifest_dir.display().to_string(),
            name: "solar-codegen".into(),
        }],
    };
    for (name, files) in RULE_SETS {
        let inputs: Vec<PathBuf> = files.iter().map(|file| isle_dir.join(file)).collect();
        for input in &inputs {
            println!("cargo:rerun-if-changed={}", input.display());
        }
        let code =
            cranelift_isle::compile::from_files(&inputs, &options).unwrap_or_else(|errors| {
                panic!("failed to compile ISLE rule set `{name}`:\n{errors:?}")
            });
        fs::write(out_dir.join(format!("{name}.isle.rs")), code)
            .unwrap_or_else(|err| panic!("failed to write ISLE rule set `{name}`: {err}"));
    }
}
