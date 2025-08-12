use solar::{
    interface::{Session, diagnostics::EmittedDiagnostics},
    sema::Compiler,
};
use std::path::Path;

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
    let _ = compiler.enter_mut(|compiler| -> solar::interface::Result<()> {
        // Parse the files.
        let mut parsing_context = compiler.parse();
        parsing_context.load_files(paths)?;
        parsing_context.parse();

        // Perform AST lowering to populate the HIR.
        compiler.lower_asts()?;

        // Inspect the HIR.
        let gcx = compiler.gcx();
        let mut contracts = gcx.hir.contracts().map(|c| c.name.to_string()).collect::<Vec<_>>();
        contracts.sort(); // No order is guaranteed.
        assert_eq!(contracts, ["AnotherCounter".to_string(), "Counter".to_string()]);

        Ok(())
    });

    // Return the emitted diagnostics as a `Result<(), _>`.
    // If any errors were emitted, this returns `Err(_)`, otherwise `Ok(())`.
    compiler.sess().emitted_errors().unwrap()
}
