//@ compile-flags: -O none -Zdump=disasm-runtime
//@ filecheck:

// `-O none` follows clang's `-O0`: its EVM IR `cfg-simplify` keeps the empty blocks that fall
// through to their target, so each of them leaves a label and a `JUMPDEST` right before the
// target's own, two adjacent `JUMPDEST`s, instead of dropping a block that may name a construct.
contract NoneAdjacentLabels {
    // CHECK: {{JUMPDEST[[:space:]]+; bb[0-9]+[[:space:]]+JUMPDEST}}
    function pick(bool c) external pure returns (bytes memory) {
        return bytes(c ? hex"abab" : hex"cdcd");
    }

    function packed(bool c) external pure returns (bytes memory) {
        return abi.encodePacked(hex"ff", c ? hex"abab" : hex"cdcd");
    }

    function discarded(bool c) external pure returns (uint256 n) {
        c ? hex"abab" : hex"cdcd"; //~ WARN: statement has no effect
        n = c ? 1 : 2;
    }
}
