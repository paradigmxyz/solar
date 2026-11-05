//@ compile-flags: --emit=bin -Onone -Zevm-ir-pipeline=none -Zdebugify=report
//@ normalize-stdout-test: "(?s).+" -> ""
// The constructor body only exists in the deployment EVM IR, so the `stackify` record must count
// the deployment code as well as the runtime code to retain every MIR origin.
contract StackifyConstructor {
    uint256 public x;

    constructor(uint256 a) {
        x = a + 1;
    }

    function f(uint256 a) external returns (uint256) {
        x += a;
        return x;
    }
}
