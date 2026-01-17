//! Simple example to test the full codegen pipeline.
#![allow(unused_crate_dependencies)]

use solar_codegen::{EvmCodegen, lower, mir::module_to_dot};
use solar_interface::Session;
use solar_sema::Compiler;
use std::{ops::ControlFlow, path::Path};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} [--dot] <file.sol>", args[0]);
        std::process::exit(1);
    }

    let (emit_dot, file_arg) = if args.get(1).map(|s| s.as_str()) == Some("--dot") {
        (true, args.get(2))
    } else {
        (false, args.get(1))
    };

    let Some(file_path) = file_arg else {
        eprintln!("Missing file argument");
        std::process::exit(1);
    };

    let path = Path::new(file_path);

    // Create session with stderr emitter so errors are visible
    let sess = Session::builder().with_stderr_emitter().build();

    // Create compiler
    let mut compiler = Compiler::new(sess);

    // Parse files
    let _ = compiler.enter_mut(|compiler| -> solar_interface::Result<_> {
        let mut parsing_context = compiler.parse();
        parsing_context.load_files([path])?;
        parsing_context.parse();
        Ok(())
    });

    // Lower to HIR and generate code
    let result = compiler.enter_mut(|compiler| -> solar_interface::Result<_> {
        // Lower ASTs to HIR
        let ControlFlow::Continue(()) = compiler.lower_asts()? else {
            return Ok(());
        };

        // Analyze
        let ControlFlow::Continue(()) = compiler.analysis()? else {
            return Ok(());
        };

        let gcx = compiler.gcx();

        // For each contract, generate MIR and bytecode
        for (contract_id, contract) in gcx.hir.contracts_enumerated() {
            println!("=== Contract: {} ===", contract.name);

            // Lower to MIR
            let mut module = lower::lower_contract(gcx, contract_id);

            if emit_dot {
                // Output DOT format CFG
                println!("{}", module_to_dot(&module));
            } else {
                println!("\n--- MIR ---");
                println!("{module}");

                // Generate bytecode
                let mut codegen = EvmCodegen::new();
                let bytecode = codegen.generate_module(&mut module);

                println!("\n--- Bytecode ({} bytes) ---", bytecode.len());
                println!("0x{}", alloy_primitives::hex::encode(&bytecode));
                println!();
            }
        }

        Ok(())
    });

    // Check for errors
    if result.is_err() {
        std::process::exit(1);
    }

    if compiler.sess().emitted_errors().is_some_and(|r| r.is_err()) {
        std::process::exit(1);
    }
}
