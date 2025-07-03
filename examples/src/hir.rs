use solar::{
    interface::{Session, diagnostics::EmittedDiagnostics},
    sema::{ParsingContext, thread_local::ThreadLocal},
};
use std::path::Path;

#[test]
fn main() -> Result<(), EmittedDiagnostics> {
    let paths = [Path::new("src/AnotherCounter.sol")];

    // Create a new session with a buffer emitter.
    // This is required to capture the emitted diagnostics and to return them at the end.
    let sess = Session::builder().with_buffer_emitter(solar::interface::ColorChoice::Auto).build();

    // Enter the context and parse the file.
    // Counter will be parsed, even if not explicitly provided, since it is a dependency.
    let _ = sess.enter_parallel(|| -> solar::interface::Result<()> {
        // Set up the parser.
        let hir_arena = ThreadLocal::new();
        let mut parsing_context = ParsingContext::new(&sess);
        parsing_context.load_files(paths)?;
        // This can be `None` if lowering is not requested in the session options.
        if let Some(gcx) = parsing_context.parse_and_lower(&hir_arena)? {
            let gcx = gcx.get();
            let mut contracts = gcx.hir.contracts().map(|c| c.name.to_string()).collect::<Vec<_>>();
            contracts.sort(); // No order is guaranteed.
            assert_eq!(contracts, ["AnotherCounter".to_string(), "Counter".to_string()]);
        }
        Ok(())
    });

    // Return the emitted diagnostics as a `Result<(), _>`.
    // If any errors were emitted, this returns `Err(_)`, otherwise `Ok(())`.
    sess.emitted_errors().unwrap()
}
