use solar_tester::foundry::{
    TestConfig, run_compilation_smoke, test_project, test_project_solar_only,
};
use std::path::Path;

const CMD: &str = env!("CARGO_BIN_EXE_solar");

fn solar() -> &'static Path {
    Path::new(CMD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithmetic() {
        test_project(solar(), "arithmetic", "tests/foundry/arithmetic");
    }

    #[test]
    fn test_control_flow() {
        test_project(solar(), "control_flow", "tests/foundry/control-flow");
    }

    #[test]
    fn test_storage() {
        test_project(solar(), "storage", "tests/foundry/storage");
    }

    #[test]
    fn test_events() {
        test_project(solar(), "events", "tests/foundry/events");
    }

    #[test]
    fn test_calls() {
        test_project(solar(), "calls", "tests/foundry/calls");
    }

    #[test]
    fn test_interfaces() {
        test_project(solar(), "interfaces", "tests/foundry/interfaces");
    }

    #[test]
    fn test_libraries() {
        test_project(solar(), "libraries", "tests/foundry/libraries");
    }

    #[test]
    fn test_constructor_args() {
        test_project(solar(), "constructor_args", "tests/foundry/constructor-args");
    }

    #[test]
    fn test_multi_return() {
        test_project(solar(), "multi_return", "tests/foundry/multi-return");
    }

    #[test]
    fn test_correctness() {
        test_project(solar(), "correctness", "tests/foundry/correctness");
    }

    #[test]
    fn test_inheritance() {
        test_project(solar(), "inheritance", "tests/foundry/inheritance");
    }

    #[test]
    fn test_stack_deep() {
        test_project_solar_only(solar(), "stack_deep", "tests/foundry/stack-deep");
    }

    #[test]
    fn test_compilation() {
        run_compilation_smoke(solar());
    }

    #[test]
    #[ignore] // Requires forge-std which is not available in CI.
    fn test_unifap_v2() {
        test_project(solar(), "unifap-v2", "tests/foundry/unifap-v2");
    }

    #[test]
    #[ignore] // Requires forge-std which is not available in CI.
    fn test_unifap_v2_create() {
        test_project(solar(), "unifap-v2-create", "tests/foundry/unifap-v2-create");
    }

    // Example: run only mint-related tests.
    #[test]
    #[ignore]
    fn test_unifap_mint_only() {
        TestConfig::new(solar(), "unifap-v2-create", "tests/foundry/unifap-v2-create")
            .test_filter("testMint")
            .run();
    }

    // Example: run only tests in a specific contract.
    #[test]
    #[ignore]
    fn test_unifap_pair_only() {
        TestConfig::new(solar(), "unifap-v2-create", "tests/foundry/unifap-v2-create")
            .contract_filter("UnifapV2Pair")
            .run();
    }

    // Example: combine test and contract filters.
    #[test]
    #[ignore]
    fn test_unifap_pair_swap() {
        TestConfig::new(solar(), "unifap-v2-create", "tests/foundry/unifap-v2-create")
            .contract_filter("UnifapV2Pair")
            .test_filter("testSwap")
            .run();
    }

    #[test]
    #[ignore] // WIP: 8 struct tests have StackUnderflow issues to fix.
    fn test_structs() {
        test_project(solar(), "structs", "tests/foundry/structs");
    }
}
