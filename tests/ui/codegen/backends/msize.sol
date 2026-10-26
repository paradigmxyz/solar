//@ revisions: evm yul sonatina llvm
//@ compile-flags: --evm-version osaka -Onone
//@[evm] compile-flags: --codegen-backend evm
//@[yul] compile-flags: --codegen-backend yul
//@[sonatina] compile-flags: --codegen-backend sonatina
//@[llvm] compile-flags: --codegen-backend llvm
//@ run-call: touched => 65568
//@ run-call: untouched => 96

// Native backends reserve private memory below the program's heap; `msize` must not observe it.
contract Msize {
    function touched() external pure returns (uint256 size) {
        assembly {
            mstore(0x10000, 1)
            size := msize()
        }
    }

    function untouched() external pure returns (uint256 size) {
        assembly {
            size := msize()
        }
    }
}
