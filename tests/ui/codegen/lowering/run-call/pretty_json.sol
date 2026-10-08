//@ compile-flags: --pretty-json
//@ run-call: add 2, 3 => 5
// With pretty JSON output, `SOLAR_RUN_CALL_MIR` still checks the call against the MIR interpreter.
contract PrettyJson {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}
