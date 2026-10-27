//@ compile-flags: -O none -Zdump=disasm-runtime
//@ filecheck:

// `-O none` runs the first EVM IR `cfg-simplify`, which drops the empty blocks that fall through
// to their target. Without it each of them would leave a label and a `JUMPDEST` right before the
// target's own, two adjacent `JUMPDEST`s.
contract NoneAdjacentLabels {
    // CHECK-NOT: {{JUMPDEST[[:space:]]+; bb[0-9]+[[:space:]]+JUMPDEST}}
    function pick(bool c) external pure returns (bytes memory) {
        return bytes(c ? hex"abab" : hex"cdcd");
    }

    function packed(bool c) external pure returns (bytes memory) {
        return abi.encodePacked(hex"ff", c ? hex"abab" : hex"cdcd");
    }

    function discarded(bool c) external pure returns (uint256 n) {
        c ? hex"abab" : hex"cdcd";
        n = c ? 1 : 2;
    }
}
