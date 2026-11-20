//@ compile-flags: -Onone
//@ run-call: add 1, 2 => 3
//@ run-call-fail: add 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => Panic(0x11)

// A user's function named like the shared panic function does not replace it: a failed check
// still reverts with its own panic code.
contract PanicHelperName {
    function panic_error_0x11() internal pure returns (uint256) {
        return 7;
    }

    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b + panic_error_0x11() - 7;
    }
}
