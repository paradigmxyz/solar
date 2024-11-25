use solar::{
    ast,
    interface::{diagnostics::EmittedDiagnostics, Session},
    parse::Parser,
};
use std::path::Path;

#[test]
fn main() -> Result<(), EmittedDiagnostics> {
    let path = Path::new("src/Counter.sol");

    // Create a new session with a buffer emitter.
    // This is required to capture the emitted diagnostics and to return them at the end.
    let sess = Session::builder().with_buffer_emitter(solar::interface::ColorChoice::Auto).build();

    // Enter the context and parse the file.
    let _ = sess.enter(|| -> solar::interface::Result<()> {
        // Set up the parser.
        let arena = ast::Arena::new();
        let mut parser = Parser::from_file(&sess, &arena, path)?;

        // Parse the file.
        let ast = parser.parse_file().map_err(|e| e.emit())?;
        println!("parsed {path:?}: {ast:#?}");
        Ok(())
    });

    // Return the emitted diagnostics as a `Result<(), _>`.
    // If any errors were emitted, this returns `Err(_)`, otherwise `Ok(())`.
    sess.emitted_errors().unwrap()
}
