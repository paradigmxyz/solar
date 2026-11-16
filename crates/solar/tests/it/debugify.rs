//! Debugify modes exercise the real MIR pass-manager audit and early-abort path.

use snapbox::{assert_data_eq, str};
use solar::{
    codegen::mir::{
        Module,
        pass::{MirPass, ModuleAnalyses, run_passes},
    },
    config::{CompileOpts, Debugify},
    evmir::debug::DebugLocKind,
    interface::{ColorChoice, Session, source_map::FileName},
    sema::{Compiler, Gcx},
};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn check_aborts_mir_pipeline_while_report_continues() {
    for mode in [None, Some(Debugify::Report), Some(Debugify::Check)] {
        let mut opts = CompileOpts::default();
        opts.unstable.debugify = mode;
        let sess = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let compiler = Compiler::new(sess);
        compiler.enter(|compiler| {
            let source = compiler
                .sess()
                .source_map()
                .new_source_file(
                    FileName::Custom("audit.mir".into()),
                    r#"@module Audit
fn @f(arg0: i256) -> i256 {
  bb0:
    v0 = add arg0, 1 !metadata(span=10..20)
    ret v0 !metadata(span=30..40)
}
"#,
                )
                .unwrap();
            let mut module = Module::parse(compiler.sess(), &source).unwrap();
            let continued = AtomicBool::new(false);
            // Parsed MIR marks unspecified locations as deliberately dropped. Introduce the
            // missing location in a test pass instead, so the manager must catch a real gap.
            assert!(run_passes(
                compiler.gcx(),
                &mut module,
                &[&LoseLocation, &ContinuePipeline(&continued)],
                Some("audit"),
            ));
            let checking = mode == Some(Debugify::Check);
            assert_eq!(continued.load(Ordering::Relaxed), !checking);
            assert_eq!(compiler.dcx().err_count(), usize::from(checking));
            let diagnostics = compiler.sess().emitted_diagnostics().unwrap().to_string();
            if checking {
                assert_data_eq!(
                    diagnostics,
                    str![[r#"
error: debugify: MIR pass `lose-location` left 1 instruction without a location


"#]]
                );
            } else {
                assert_data_eq!(diagnostics, str![""]);
            }
        });
    }
}

struct LoseLocation;

impl MirPass for LoseLocation {
    fn name(&self) -> &'static str {
        "lose-location"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, _analyses: &mut ModuleAnalyses) -> bool {
        let function = module.functions.iter_mut().next().unwrap();
        let instruction = function.instructions().next().unwrap();
        function.inst_mut(instruction).metadata.drop_debug_loc(DebugLocKind::Normal);
        true
    }
}

struct ContinuePipeline<'a>(&'a AtomicBool);

impl MirPass for ContinuePipeline<'_> {
    fn name(&self) -> &'static str {
        "continue-pipeline"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        _module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        self.0.store(true, Ordering::Relaxed);
        false
    }
}
