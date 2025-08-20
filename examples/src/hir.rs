use solar::{
    interface::{Session, diagnostics::EmittedDiagnostics},
    sema::Compiler,
};
use std::{ops::ControlFlow, path::Path};

#[test]
fn main() -> Result<(), EmittedDiagnostics> {
    let paths = [Path::new("src/AnotherCounter.sol")];

    // Create a new session with a buffer emitter.
    // This is required to capture the emitted diagnostics and to return them at the end.
    let sess = Session::builder().with_buffer_emitter(solar::interface::ColorChoice::Auto).build();

    // Create a new compiler.
    let mut compiler = Compiler::new(sess);

    // Enter the context and parse the file.
    // Counter will be parsed, even if not explicitly provided, since it is a dependency.
    let contracts = compiler.enter_mut(|compiler| -> solar::interface::Result<_> {
        // Parse the files.
        let mut parsing_context = compiler.parse();
        parsing_context.load_files(paths)?;
        parsing_context.parse();

        // Perform AST lowering to populate the HIR.
        let ControlFlow::Continue(()) = compiler.lower_asts()? else {
            // Can't continue because HIR was not populated,
            // possibly because it was requested in `Session` with `stop_after`.
            return Ok(vec![]);
        };

        // Inspect the HIR.
        let gcx = compiler.gcx();
        let contracts = gcx.hir.contracts().map(|c| c.name.to_string()).collect::<Vec<_>>();
        Ok(contracts)
    });
    if let Ok(mut contracts) = contracts {
        // No order is guaranteed.
        contracts.sort();
        assert_eq!(contracts, ["AnotherCounter".to_string(), "Counter".to_string()]);
    }

    // Return the emitted diagnostics as a `Result<(), _>`.
    // If any errors were emitted, this returns `Err(_)`, otherwise `Ok(())`.
    // Note that this discards warnings and other non-error diagnostics.
    compiler.sess().emitted_errors().unwrap()
}
