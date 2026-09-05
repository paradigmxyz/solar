//@ codegen-matrix: standard debug
//@[debug] compile-flags: --revert-strings debug
//@ run-call: twice 21 => 42
//@ run-call: twice 21; value=1 => 42
//@ run-call: 0xcbdd72a40000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: 0xcbdd72a40000000000000000000000000000000000000000000000000000000000000000; value=1 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: probe => 96, 0
//@[none] run-call-fail: 0x14f9a01b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[gas] run-call-fail: 0x14f9a01b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[size] run-call-fail: 0x14f9a01b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[mir] run-call-fail: 0x14f9a01b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => 0x
//@[debug] run-call-fail: 0x14f9a01b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001 => Error("Non-view function of library called without DELEGATECALL")
//@[debug] run-call-fail: 0xdeadbeef => Error("Contract does not have fallback nor receive functions")

// A library's non-view external functions only run through `DELEGATECALL`. Like solc, the
// runtime compares `address()` against the library's own deployment address, which the creation
// code patches into the runtime code like an immutable, and rejects a direct call; `--revert-strings
// debug` names the check. View and pure functions accept direct calls, and no library function
// checks `callvalue`, since a `DELEGATECALL` sees the caller's value. Functions taking storage
// pointers are not part of the ABI, so `bump` and `peek` are called with raw calldata.
library Lib {
    function bump(uint256[] storage a, uint256 v) external returns (uint256) {
        a.push(v);
        return a.length;
    }

    function peek(uint256[] storage a) external view returns (uint256) {
        return a.length;
    }

    function twice(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }

    // The guard is computed inside each guarded case, so an unguarded route sees the memory
    // the dispatch left behind exactly as solc's does.
    function probe() external pure returns (uint256 size, uint256 word) {
        assembly {
            size := msize()
            word := mload(0x80)
        }
    }
}
