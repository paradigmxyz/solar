//@ codegen-matrix: standard ir irsize
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --check-prefix=GAS
//@[irsize] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[irsize] filecheck: --check-prefix=SIZE
// Standalone resident-expression selection is covered in stack_word_resident.sol.
// GAS-LABEL: @module StackWords_runtime
// In gas mode each dispatched function returns on its own, which saves the jump and the
// swaps a shared tail needs.
// GAS: push 36
// GAS-NEXT: calldataload
// GAS-NEXT: dup 1
// GAS-NEXT: dup 1
// GAS-NEXT: push 4
// GAS-NEXT: calldataload
// GAS-NEXT: swap 3
// GAS-NEXT: dup 4
// GAS-NEXT: and
// GAS-NEXT: push 0
// GAS-NEXT: mstore
// GAS-NEXT: dup 3
// GAS-NEXT: or
// GAS-NEXT: push 32
// GAS-NEXT: mstore
// GAS-NEXT: add
// GAS-NEXT: push 64
// GAS-NEXT: mstore
// GAS-NEXT: push 96
// GAS-NEXT: push 0
// GAS-NEXT: return
// SIZE-LABEL: @module StackWords_runtime
// In size mode the dispatched functions share their ABI return tail.
// SIZE: jump [[JOIN:bb[0-9]+]]
// SIZE-NEXT: [[JOIN]]:
// SIZE-NEXT: swap 3
// SIZE-NEXT: and
// SIZE-NEXT: jump [[TAIL:bb[0-9]+]]
// SIZE-NEXT: [[TAIL]]:
// SIZE-NEXT: push 0
// SIZE-NEXT: mstore
// SIZE-NEXT: push 32
// SIZE-NEXT: mstore
// SIZE-NEXT: push 64
// SIZE-NEXT: mstore
// SIZE-NEXT: push 96
// SIZE-NEXT: push 0
// SIZE-NEXT: return
// SIZE-NOT: {{^ *return$}}
// SIZE: jump [[TAIL]]
//@ run-call: sum 9, 4 => 0, 13, 13
//@ run-call: sum 7, 7 => 7, 7, 14
//@ run-call: xor 9, 4 => 0, 13, 13
//@ run-call: xor 7, 7 => 7, 7, 0
//@ run-call: merged 9, 4 => 0, 13, 13
//@ run-call: merged 7, 7 => 7, 0, 7
//@ run-call: masked 9, 4 => 13, 13, 0
//@ run-call: masked 7, 7 => 7, 0, 7
//@ run-call: sum 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
contract StackWords {
    function sum(uint256 x, uint256 y) public pure returns (uint256 common, uint256 either, uint256 total) {
        unchecked { common = x & y; either = x | y; total = x + y; }
    }
    function xor(uint256 x, uint256 y) public pure returns (uint256 common, uint256 either, uint256 total) {
        common = x & y;
        either = x | y;
        total = x ^ y;
    }
    function merged(uint256 x, uint256 y) public pure returns (uint256 common, uint256 different, uint256 total) {
        common = x & y;
        different = x ^ y;
        total = x | y;
    }
    function masked(uint256 x, uint256 y) public pure returns (uint256 either, uint256 different, uint256 total) {
        either = x | y;
        different = x ^ y;
        total = x & y;
    }
}
