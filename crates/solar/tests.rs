#![allow(unused_crate_dependencies)]

const CMD: &str = env!("CARGO_BIN_EXE_solar");

fn main() -> std::process::ExitCode {
    let mut args = std::env::args_os();
    if args.nth(1).as_deref() == Some(solar_tester::STANDARD_JSON_ARG.as_ref()) {
        let Some(input) = args.next() else {
            eprintln!("missing standard JSON test input");
            return std::process::ExitCode::FAILURE;
        };
        return match solar_tester::run_standard_json_test(CMD.as_ref(), input.as_ref()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e:?}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    match solar_tester::run_tests(CMD.as_ref()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:?}");
            std::process::ExitCode::FAILURE
        }
    }
}
