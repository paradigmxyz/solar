//! Exercise a wrong-code edit through the real MIR pass manager and Lean checker.

use snapbox::{assert_data_eq, str};
use solar::{
    codegen::mir::{
        Module,
        pass::{MirPass, ModuleAnalyses, run_passes},
    },
    config::CompileOpts,
    interface::{
        ColorChoice, Session,
        source_map::{FileLoader, FileName, RealFileLoader},
    },
    sema::{Compiler, Gcx},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
#[ignore = "requires the solar-tv Lean toolchain and SOLAR / SOLAR_TV executables"]
fn counterexample_stops_pipeline_and_replays() {
    let audit = tempfile::tempdir().unwrap();
    let mut opts = CompileOpts::default();
    opts.unstable.solar_tv = Some(audit.path().to_owned());
    let session = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
    Compiler::new(session).enter(|compiler| {
        let file = compiler
            .sess()
            .source_map()
            .new_source_file(
                FileName::Custom("original.mir".into()),
                include_str!("../../../../solar-tv/examples/mir/dce/after.mir"),
            )
            .unwrap();
        let mut module = Module::parse(compiler.sess(), &file).unwrap();
        let _ = run_passes(compiler.gcx(), &mut module, &[&WrongReturn, &Unreachable], None);
        assert_eq!(compiler.dcx().err_count(), 1);
    });
    let mut pairs = pairs(audit.path());
    let pair = pairs.next().unwrap();
    assert!(pairs.next().is_none());
    let report = RealFileLoader.load_file(&pair.join("result.txt")).unwrap();
    assert_data_eq!(
        report.lines().next().unwrap(),
        str!["solar-tv: mir pass `wrong-return` on `Dce`: counterexample"]
    );
    let replay = Command::new(std::env::var_os("SOLAR_TV").unwrap())
        .arg("mir")
        .arg(pair.join("before.mir"))
        .arg(pair.join("after.mir"))
        .arg("--solar")
        .arg(env!("CARGO_BIN_EXE_solar"))
        .output()
        .unwrap();
    assert_eq!(replay.status.code(), Some(1));
    assert_data_eq!(
        String::from_utf8(replay.stdout)
            .unwrap()
            .replace(&pair.canonicalize().unwrap().display().to_string(), "PAIR")
            .replace(&pair.display().to_string(), "PAIR")
            .replace(&std::env::var("SOLAR_TV").unwrap(), "CHECKER")
            .replace(
                &Path::new(env!("CARGO_BIN_EXE_solar"))
                    .with_file_name(format!("solar-tv-replay{}", std::env::consts::EXE_SUFFIX))
                    .display()
                    .to_string(),
                "REPLAY"
            )
            .replace(env!("CARGO_BIN_EXE_solar"), "SOLAR"),
        str![[r#"
counterexample
  MIR function @with_dead_code
  arg0 = 0
  before = 1
  after  = 0

Replay checks
  Lean kernel: confirmed
  Rust interpreter: matched

Difference
  Returned value changed

Changed IR
--- before
+++ after
@@ -3,4 +3,4 @@
   bb0:
     v0 = add arg0, 1
-    ret v0
+    ret arg0
 }

Replay
  CHECKER mir PAIR/before.mir PAIR/after.mir --solar SOLAR --timeout 60 --evm-version cancun --replay REPLAY

"#]]
    );
}

struct WrongReturn;

impl MirPass for WrongReturn {
    fn name(&self) -> &'static str {
        "wrong-return"
    }
    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, _: &mut ModuleAnalyses) -> bool {
        let f = module.functions.iter_mut().next().unwrap();
        let arg = f.params.indices().next().unwrap();
        let input = f.alloc_arg(arg);
        let inst = f.instructions().next().unwrap();
        let sum = f.inst_result_value(inst).unwrap();
        // ret (arg0 + 1) => ret arg0
        f.replace_uses(&[(sum, input)].into_iter().collect());
        // Even a pass that incorrectly claims no change must be checked.
        false
    }
}

struct Unreachable;

impl MirPass for Unreachable {
    fn name(&self) -> &'static str {
        "unreachable"
    }
    fn is_required(&self) -> bool {
        true
    }
    fn run_pass(&self, _: Gcx<'_>, _: &mut Module, _: &mut ModuleAnalyses) -> bool {
        panic!("pipeline continued after a counterexample")
    }
}

#[test]
fn checker_launch_failure_keeps_both_ir_inputs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../solar-tv/examples");
    for (input, layer, flag, pass) in [
        ("dce/before.mir", "mir", "mir", "dce"),
        ("peephole/before.evmir", "evmir", "evm-ir", "peephole"),
    ] {
        let audit = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_solar"))
            .arg(root.join(layer).join(input))
            .arg(format!("-Z{flag}-pipeline={pass},none"))
            .arg("-Zpass-diff")
            .arg(format!("-Zsolar-tv={}", audit.path().display()))
            .env("SOLAR_TV", audit.path().join("absent"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        // A diff from the second pipeline entry would reach stdout if execution continued.
        assert_data_eq!(String::from_utf8(output.stdout).unwrap(), str![""]);
        let mut pairs = pairs(audit.path());
        let path = pairs.next().unwrap();
        assert!(pairs.next().is_none());
        let read = |file| RealFileLoader.load_file(&path.join(file)).unwrap();
        assert_ne!(read(format!("before.{layer}")), read(format!("after.{layer}")));
        let report = read("result.txt".into());
        assert!(report.lines().next().unwrap().ends_with("checker failed"));
    }
}

fn pairs(directory: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap() != "audits")
}
