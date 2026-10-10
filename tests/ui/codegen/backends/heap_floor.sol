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
//@ run-call: runtime() => true
//@ run-call: deployed() => true

contract HeapFloor {
    bool public deployed;

    constructor() {
        deployed = allocatesAboveFloor();
    }

    function runtime() external pure returns (bool) {
        return allocatesAboveFloor();
    }

    function allocatesAboveFloor() internal pure returns (bool) {
        assembly {
            mstore(0x40, 0)
        }
        bytes memory data = new bytes(32);
        uint256 pointer;
        assembly {
            pointer := data
        }
        return pointer >= 0x80;
    }
}
