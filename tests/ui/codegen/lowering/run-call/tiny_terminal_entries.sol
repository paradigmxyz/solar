//@ codegen-matrix: standard
//@ run-call: value 7 => 7
//@ run-call: checked 7 => 8
//@ run-call-fail: checked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: 0xffffffff => 0x

// Dispatcher and argument checks share tiny reverting exits without changing
// successful calls, unknown-selector reverts, or checked-arithmetic panic data.
contract TinyTerminalEntries {
    function value(uint256 input) external pure returns (uint256) {
        return input;
    }

    function checked(uint256 input) external pure returns (uint256) {
        return input + 1;
    }
}
