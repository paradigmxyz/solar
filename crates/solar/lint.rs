#![allow(unused_crate_dependencies)]

use solar::{
    ast,
    interface::{
        ColorChoice, Session, Span,
        diagnostics::{Applicability, Level},
    },
    lint::{
        EarlyLintPass, LateLintPass, LateLintVisitor, Lint, LintContext, LintPolicy, LintRegistry,
        LintRunContext, LintRunError, LintSource, LintSuite, ProjectLintContext, ProjectLintPass,
        ProjectSource, Suggestion, run_lints,
    },
    sema::{Compiler, Gcx, hir},
};
use std::{
    ops::ControlFlow,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct TestLint {
    id: &'static str,
    level: Level,
}

impl Lint for TestLint {
    fn id(&self) -> &'static str {
        self.id
    }

    fn level(&self) -> Level {
        self.level
    }

    fn description(&self) -> &'static str {
        "test lint"
    }

    fn help(&self) -> &'static str {
        "https://example.com/lint"
    }
}

static EARLY: TestLint = TestLint { id: "early", level: Level::Warning };
static VARIABLE: TestLint = TestLint { id: "variable", level: Level::Note };
static SUGGESTION: TestLint = TestLint { id: "suggestion", level: Level::Warning };
static SUPPRESSED: TestLint = TestLint { id: "suppressed", level: Level::Warning };
static LATE: TestLint = TestLint { id: "late", level: Level::Warning };
static PROJECT: TestLint = TestLint { id: "project", level: Level::Warning };
static EARLY_IDS: &[&str] = &[EARLY.id, VARIABLE.id, SUGGESTION.id, SUPPRESSED.id];
static SINGLE_EARLY_ID: &[&str] = &[EARLY.id];
static LATE_IDS: &[&str] = &[LATE.id];
static PROJECT_IDS: &[&str] = &[PROJECT.id];

#[derive(Default)]
struct TestPolicy;

impl LintPolicy for TestPolicy {
    fn is_lint_enabled(&self, _id: &str) -> bool {
        true
    }

    fn is_lint_suppressed(&self, id: &str, _span: Span) -> bool {
        id == SUPPRESSED.id
    }
}

struct TestEarlyPass {
    configured: usize,
    visited: bool,
}

impl<'ast> EarlyLintPass<'ast> for TestEarlyPass {
    fn check_item_contract(
        &mut self,
        ctx: &LintContext<'_, '_>,
        contract: &'ast ast::ItemContract<'ast>,
    ) {
        assert_eq!(self.configured, 42);
        assert!(!self.visited);
        self.visited = true;

        ctx.emit(&EARLY, contract.name.span);
        ctx.emit(&EARLY, contract.name.span);
        ctx.emit_with_msg(&VARIABLE, contract.name.span, "variable message");
        ctx.emit_with_suggestion(
            &SUGGESTION,
            contract.name.span,
            Suggestion::fix("Renamed".to_owned(), Applicability::MaybeIncorrect),
        );
        ctx.emit(&SUPPRESSED, contract.name.span);
    }
}

#[derive(Default)]
struct TestLatePass;

impl<'hir> LateLintPass<'hir> for TestLatePass {
    fn check_contract(
        &mut self,
        ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'hir>,
        _hir: &'hir hir::Hir<'hir>,
        contract: &'hir hir::Contract<'hir>,
    ) {
        ctx.emit(&LATE, contract.span);
    }
}

#[derive(Debug, Default)]
struct HookCounts {
    nested_item: usize,
    nested_contract: usize,
    nested_function: usize,
    nested_var: usize,
    modifier: usize,
    call_args: usize,
}

struct RecordingLatePass {
    counts: Arc<Mutex<HookCounts>>,
}

impl RecordingLatePass {
    fn record(&self, update: impl FnOnce(&mut HookCounts)) {
        update(&mut self.counts.lock().unwrap());
    }
}

impl<'hir> LateLintPass<'hir> for RecordingLatePass {
    fn check_nested_item(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _hir: &'hir hir::Hir<'hir>,
        _id: hir::ItemId,
    ) {
        self.record(|counts| counts.nested_item += 1);
    }

    fn check_nested_contract(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'hir>,
        _hir: &'hir hir::Hir<'hir>,
        _id: hir::ContractId,
    ) {
        self.record(|counts| counts.nested_contract += 1);
    }

    fn check_nested_function(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _hir: &'hir hir::Hir<'hir>,
        _id: hir::FunctionId,
    ) {
        self.record(|counts| counts.nested_function += 1);
    }

    fn check_nested_var(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _hir: &'hir hir::Hir<'hir>,
        _id: hir::VariableId,
    ) {
        self.record(|counts| counts.nested_var += 1);
    }

    fn check_modifier(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _hir: &'hir hir::Hir<'hir>,
        _modifier: &'hir hir::Modifier<'hir>,
    ) {
        self.record(|counts| counts.modifier += 1);
    }

    fn check_call_args(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _hir: &'hir hir::Hir<'hir>,
        _args: &'hir hir::CallArgs<'hir>,
    ) {
        self.record(|counts| counts.call_args += 1);
    }
}

struct RecordingEarlyPass {
    value: usize,
    order: Arc<Mutex<Vec<usize>>>,
}

impl<'ast> EarlyLintPass<'ast> for RecordingEarlyPass {
    fn check_item_contract(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _contract: &'ast ast::ItemContract<'ast>,
    ) {
        self.order.lock().unwrap().push(self.value);
    }
}

struct TestProjectPass {
    expected_sources: usize,
}

impl<'ast> ProjectLintPass<'ast> for TestProjectPass {
    fn check_project(&mut self, ctx: &ProjectLintContext<'_, '_>, sources: &[ProjectSource<'ast>]) {
        assert_eq!(sources.len(), self.expected_sources);
        let source = &sources[0];
        let span = source.ast.items.first().unwrap().span;
        ctx.emit(source, &PROJECT, span);
        ctx.emit(source, &PROJECT, span);
    }
}

struct TestSuite {
    registry: LintRegistry,
    registered: Arc<AtomicUsize>,
    early_created: Arc<AtomicUsize>,
    late_created: Arc<AtomicUsize>,
    project_created: Arc<AtomicUsize>,
}

impl TestSuite {
    fn new(expected_sources: usize) -> Self {
        let registered = Arc::new(AtomicUsize::new(0));
        let early_created = Arc::new(AtomicUsize::new(0));
        let late_created = Arc::new(AtomicUsize::new(0));
        let project_created = Arc::new(AtomicUsize::new(0));
        let mut registry = LintRegistry::new();
        registered.fetch_add(1, Ordering::Relaxed);

        let created = early_created.clone();
        registry.register_early_pass(EARLY_IDS, move || {
            created.fetch_add(1, Ordering::Relaxed);
            TestEarlyPass { configured: 42, visited: false }
        });

        let created = late_created.clone();
        registry.register_late_pass(LATE_IDS, move || {
            created.fetch_add(1, Ordering::Relaxed);
            TestLatePass
        });

        let created = project_created.clone();
        registry.register_project_pass(PROJECT_IDS, move || {
            created.fetch_add(1, Ordering::Relaxed);
            TestProjectPass { expected_sources }
        });

        Self { registry, registered, early_created, late_created, project_created }
    }
}

impl LintSuite for TestSuite {
    fn registry(&self) -> &LintRegistry {
        &self.registry
    }

    fn source_policy(&self, _source: LintSource<'_, '_>) -> Arc<dyn LintPolicy> {
        Arc::new(TestPolicy)
    }

    fn project_policy(&self) -> Arc<dyn LintPolicy> {
        Arc::new(TestPolicy)
    }
}

#[derive(Default)]
struct DisabledPolicy;

impl LintPolicy for DisabledPolicy {
    fn is_lint_enabled(&self, _id: &str) -> bool {
        false
    }

    fn is_lint_suppressed(&self, _id: &str, _span: Span) -> bool {
        false
    }
}

#[derive(Default)]
struct ProjectSuppressingPolicy;

impl LintPolicy for ProjectSuppressingPolicy {
    fn is_lint_enabled(&self, _id: &str) -> bool {
        true
    }

    fn is_lint_suppressed(&self, id: &str, _span: Span) -> bool {
        id == PROJECT.id
    }
}

struct RegistrySuite {
    registry: LintRegistry,
    source_policy: Arc<dyn LintPolicy>,
    project_policy: Arc<dyn LintPolicy>,
}

impl LintSuite for RegistrySuite {
    fn registry(&self) -> &LintRegistry {
        &self.registry
    }

    fn source_policy(&self, _source: LintSource<'_, '_>) -> Arc<dyn LintPolicy> {
        self.source_policy.clone()
    }

    fn project_policy(&self) -> Arc<dyn LintPolicy> {
        self.project_policy.clone()
    }
}

fn compiler(sources: &[(&str, &str)]) -> Compiler {
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    let mut compiler = Compiler::new(sess);
    compiler.enter_mut(|compiler| {
        let mut parser = compiler.parse();
        for &(path, source) in sources {
            let file =
                compiler.sess().source_map().new_source_file(PathBuf::from(path), source).unwrap();
            parser.add_file(file);
        }
        parser.parse();
        assert_eq!(compiler.lower_asts(), Ok(ControlFlow::Continue(())));
        assert_eq!(compiler.analysis(), Ok(ControlFlow::Continue(())));
    });
    compiler
}

fn run(compiler: &Compiler, suite: &TestSuite, targets: &[PathBuf]) {
    let warnings_before = compiler.dcx().warn_count();
    let notes_before = compiler.dcx().note_count();
    let result = compiler
        .enter(|compiler| {
            run_lints(
                suite,
                LintRunContext {
                    gcx: compiler.gcx(),
                    targets,
                    with_description: true,
                    with_ansi_help: false,
                },
            )
        })
        .unwrap();
    assert_eq!(result.visited_sources, targets.len());
    assert_eq!(compiler.dcx().warn_count() - warnings_before, 5 * targets.len());
    assert_eq!(compiler.dcx().note_count() - notes_before, targets.len());
}

#[test]
fn runs_downstream_lint_suite() {
    let compiler =
        compiler(&[("A.sol", "contract A {}"), ("Excluded.sol", "contract Excluded {}")]);
    let targets = [PathBuf::from("A.sol")];
    let suite = TestSuite::new(1);

    run(&compiler, &suite, &targets);
    assert_eq!(suite.early_created.load(Ordering::Relaxed), 1);
    assert_eq!(suite.late_created.load(Ordering::Relaxed), 1);
    assert_eq!(suite.project_created.load(Ordering::Relaxed), 1);
}

#[test]
fn configures_diagnostic_presentation_per_run() {
    let suite = TestSuite::new(1);

    for (with_description, with_ansi_help) in [(false, false), (true, true)] {
        let compiler = compiler(&[("A.sol", "contract A {}")]);
        let targets = [PathBuf::from("A.sol")];

        compiler
            .enter(|compiler| {
                run_lints(
                    &suite,
                    LintRunContext {
                        gcx: compiler.gcx(),
                        targets: &targets,
                        with_description,
                        with_ansi_help,
                    },
                )
            })
            .unwrap();
        let diagnostics = compiler.dcx().emitted_diagnostics().unwrap().to_string();
        assert_eq!(diagnostics.contains("test lint"), with_description);
    }
}

#[test]
fn creates_fresh_passes_for_repeated_runs() {
    let compiler = compiler(&[("A.sol", "contract A {}")]);
    let targets = [PathBuf::from("A.sol")];
    let suite = TestSuite::new(1);

    run(&compiler, &suite, &targets);
    run(&compiler, &suite, &targets);
    assert_eq!(suite.registered.load(Ordering::Relaxed), 1);
    assert_eq!(suite.early_created.load(Ordering::Relaxed), 2);
    assert_eq!(suite.late_created.load(Ordering::Relaxed), 2);
    assert_eq!(suite.project_created.load(Ordering::Relaxed), 2);
}

#[test]
fn creates_fresh_passes_for_concurrent_runs() {
    let suite = Arc::new(TestSuite::new(1));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let suite = suite.clone();
            std::thread::spawn(move || {
                let compiler = compiler(&[("A.sol", "contract A {}")]);
                let targets = [PathBuf::from("A.sol")];
                run(&compiler, &suite, &targets);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(suite.early_created.load(Ordering::Relaxed), 2);
    assert_eq!(suite.late_created.load(Ordering::Relaxed), 2);
    assert_eq!(suite.project_created.load(Ordering::Relaxed), 2);
}

#[test]
fn validates_all_targets_before_running_passes() {
    let compiler = compiler(&[("A.sol", "contract A {}")]);
    let targets = [PathBuf::from("A.sol"), PathBuf::from("Missing.sol")];
    let suite = TestSuite::new(0);

    let result = compiler.enter(|compiler| {
        run_lints(
            &suite,
            LintRunContext {
                gcx: compiler.gcx(),
                targets: &targets,
                with_description: true,
                with_ansi_help: false,
            },
        )
    });
    assert_eq!(result, Err(LintRunError::MissingAstSource(PathBuf::from("Missing.sol"))));
    assert_eq!(suite.early_created.load(Ordering::Relaxed), 0);
    assert_eq!(suite.late_created.load(Ordering::Relaxed), 0);
    assert_eq!(suite.project_created.load(Ordering::Relaxed), 0);
    assert_eq!(compiler.dcx().warn_count(), 0);
    assert_eq!(compiler.dcx().note_count(), 0);
}

#[test]
fn calls_late_hooks_for_nested_items_modifiers_and_call_args() {
    let source = r#"
        pragma solidity ^0.8.20;

        contract Base {
            function hook(uint256 value) internal pure returns (uint256) {
                return value;
            }
        }

        contract Test is Base {
            uint256 stored;

            modifier gated(uint256 amount) {
                _;
            }

            function run(uint256 amount) public gated(amount) returns (uint256) {
                return hook(amount + stored);
            }
        }
    "#;
    let compiler = compiler(&[("Test.sol", source)]);
    let counts = Arc::new(Mutex::new(HookCounts::default()));

    compiler.enter(|compiler| {
        let gcx = compiler.gcx();
        let source_id = gcx.hir.source_ids().next().expect("expected one lowered source");
        let policy = TestPolicy;
        let ctx = LintContext::new(gcx.sess, &policy, false, false, None);
        let mut passes: Vec<Box<dyn LateLintPass<'_>>> =
            vec![Box::new(RecordingLatePass { counts: counts.clone() })];
        let mut visitor = LateLintVisitor::new(&ctx, &mut passes, gcx, &gcx.hir);
        let _ = hir::Visit::visit_nested_source(&mut visitor, source_id);
    });

    let counts = counts.lock().unwrap();
    assert!(counts.nested_item > 0, "expected nested item hook to run");
    assert!(counts.nested_contract > 0, "expected nested contract hook to run");
    assert!(counts.nested_function > 0, "expected nested function hook to run");
    assert!(counts.nested_var > 0, "expected nested var hook to run");
    assert!(counts.modifier > 0, "expected modifier hook to run");
    assert!(counts.call_args > 0, "expected call args hook to run");
}

#[test]
fn disabled_registrations_do_not_create_passes() {
    let created = Arc::new(AtomicUsize::new(0));
    let mut registry = LintRegistry::new();

    let count = created.clone();
    registry.register_early_pass(SINGLE_EARLY_ID, move || {
        count.fetch_add(1, Ordering::Relaxed);
        TestEarlyPass { configured: 42, visited: false }
    });
    let count = created.clone();
    registry.register_late_pass(LATE_IDS, move || {
        count.fetch_add(1, Ordering::Relaxed);
        TestLatePass
    });
    let count = created.clone();
    registry.register_project_pass(PROJECT_IDS, move || {
        count.fetch_add(1, Ordering::Relaxed);
        TestProjectPass { expected_sources: 1 }
    });

    let suite = RegistrySuite {
        registry,
        source_policy: Arc::new(DisabledPolicy),
        project_policy: Arc::new(DisabledPolicy),
    };
    let compiler = compiler(&[("A.sol", "contract A {}")]);
    let targets = [PathBuf::from("A.sol")];
    compiler
        .enter(|compiler| {
            run_lints(
                &suite,
                LintRunContext {
                    gcx: compiler.gcx(),
                    targets: &targets,
                    with_description: false,
                    with_ansi_help: false,
                },
            )
        })
        .unwrap();

    assert_eq!(created.load(Ordering::Relaxed), 0);
}

#[test]
fn executes_duplicate_passes_in_registration_order() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut registry = LintRegistry::new();
    for value in [1, 2] {
        let order = order.clone();
        registry.register_early_pass(SINGLE_EARLY_ID, move || RecordingEarlyPass {
            value,
            order: order.clone(),
        });
    }
    let suite = RegistrySuite {
        registry,
        source_policy: Arc::new(TestPolicy),
        project_policy: Arc::new(TestPolicy),
    };
    let compiler = compiler(&[("A.sol", "contract A {}")]);
    let targets = [PathBuf::from("A.sol")];
    compiler
        .enter(|compiler| {
            run_lints(
                &suite,
                LintRunContext {
                    gcx: compiler.gcx(),
                    targets: &targets,
                    with_description: false,
                    with_ansi_help: false,
                },
            )
        })
        .unwrap();

    assert_eq!(*order.lock().unwrap(), [1, 2]);
}

#[test]
fn project_diagnostics_use_source_suppression_policy() {
    let created = Arc::new(AtomicUsize::new(0));
    let count = created.clone();
    let mut registry = LintRegistry::new();
    registry.register_project_pass(PROJECT_IDS, move || {
        count.fetch_add(1, Ordering::Relaxed);
        TestProjectPass { expected_sources: 1 }
    });
    let suite = RegistrySuite {
        registry,
        source_policy: Arc::new(ProjectSuppressingPolicy),
        project_policy: Arc::new(TestPolicy),
    };
    let compiler = compiler(&[("A.sol", "contract A {}")]);
    let targets = [PathBuf::from("A.sol")];
    compiler
        .enter(|compiler| {
            run_lints(
                &suite,
                LintRunContext {
                    gcx: compiler.gcx(),
                    targets: &targets,
                    with_description: true,
                    with_ansi_help: false,
                },
            )
        })
        .unwrap();

    assert_eq!(created.load(Ordering::Relaxed), 1);
    assert_eq!(compiler.dcx().warn_count(), 0);
}
