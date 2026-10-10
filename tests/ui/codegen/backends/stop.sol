//@ revisions: evm yul sonatina sir llvm evm_none yul_none sonatina_none sir_none llvm_none evm_size yul_size sonatina_size sir_size llvm_size
//@ compile-flags: --evm-version osaka
//@[evm] compile-flags: --codegen-backend evm -Ogas
//@[yul] compile-flags: --codegen-backend yul -Ogas
//@[sonatina] compile-flags: --codegen-backend sonatina -Ogas
//@[sir] compile-flags: --codegen-backend sir -Ogas
//@[llvm] compile-flags: --codegen-backend llvm -Ogas
//@[evm_none] compile-flags: --codegen-backend evm -Onone
//@[evm_size] compile-flags: --codegen-backend evm -Osize
//@[yul_none] compile-flags: --codegen-backend yul -Onone
//@[yul_size] compile-flags: --codegen-backend yul -Osize
//@[sonatina_none] compile-flags: --codegen-backend sonatina -Onone
//@[sonatina_size] compile-flags: --codegen-backend sonatina -Osize
//@[sir_none] compile-flags: --codegen-backend sir -Onone
//@[sir_size] compile-flags: --codegen-backend sir -Osize
//@[llvm_none] compile-flags: --codegen-backend llvm -Onone
//@[llvm_size] compile-flags: --codegen-backend llvm -Osize
//@ run-call: 0xfb1fad500000000000000000000000000000000000000000000000000000000000000000 => 0x
//@ run-call: halt 1 => 1

contract Stop {
    function halt(uint256 input) external pure returns (uint256) {
        stopIfZero(input);
        return input;
    }

    function stopIfZero(uint256 input) internal pure {
        assembly {
            if iszero(input) { stop() }
        }
    }
}
