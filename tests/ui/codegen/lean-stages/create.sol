//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// Creating a contract copies its creation code, program data after the runtime code, with
// `CODECOPY` from the offset and of the size assembly fixes, which the stages take from the
// assembled code, and `CREATE` and `CREATE2` take the new address from the oracle. Bytecode.lean
// leaves the program data out of the code it decodes.

contract Child {}

contract LeanStagesCreate {
    function f() external returns (address) {
        return address(new Child());
    }

    function g(bytes32 salt) external returns (address) {
        return address(new Child{salt: salt}());
    }
}
