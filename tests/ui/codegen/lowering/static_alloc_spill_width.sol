//@ compile-flags: -O size -Zdump=disasm-runtime -Zlegacy-stack-lowering
//@ filecheck:
//@ run-call: f => 5, 6, 0

// Placing `other` in front of the spill slots would move the loop index slot
// from 0xe0 to 0x120 and widen each of its pushes to `PUSH2`. The layout puts
// `other` after the spills instead, so the index keeps its `PUSH1` address.
contract C {
    uint256[3][] private triples;

    function f() external returns (uint256, uint256, uint256) {
        uint256[2] memory pair = [uint256(5), 6];
        uint256[2] memory other = [uint256(9), 9];
        other;
        triples.push(pair);
        return (triples[0][0], triples[0][1], triples[0][2]);
    }
}

// CHECK: PUSH2 0x0180
// CHECK-NEXT: PUSH1 0x09
// CHECK-LABEL: ; bb0
// CHECK-NEXT: JUMPDEST
// CHECK-NEXT: PUSH1 0x03
// CHECK-NEXT: PUSH1 0xe0
// CHECK-NEXT: MLOAD
