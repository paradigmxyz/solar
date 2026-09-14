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
//@ run-call: shifts 1, 4 => 16
//@ run-call: shifts 1, 256 => 0
//@ run-call: divide 42, 2 => 21
//@ run-call: divide 42, 0 => 0
//@ run-call: loop 5 => 120
//@ run-call: loop 0 => 1
//@ run-call: store 42 => 42
//@ run-call: initial => 7
//@ run-call: scratch 1 => 123
//@ run-call: scratch 2 => 123
//@ run-call: scratch 3 => 123
//@ run-call: voidCall 5 => 6
//@ run-call-fail: fail

contract Scalars {
    uint256 value = 7;

    function initial() external view returns (uint256) { return value; }

    function shifts(uint256 input, uint256 bits) external pure returns (uint256 result) {
        assembly { result := or(shl(bits, input), shr(bits, input)) }
    }

    function divide(uint256 lhs, uint256 rhs) external pure returns (uint256 result) {
        assembly { result := div(lhs, rhs) }
    }

    function loop(uint256 count) external pure returns (uint256 result) {
        result = 1;
        for (uint256 i = 2; i <= count; ++i) result *= i;
    }

    function store(uint256 input) external returns (uint256) {
        value = input;
        return value;
    }

    function scratch(uint256 n) external returns (uint256 result) {
        assembly {
            mstore(0, 123)
            switch n
            case 1 { sstore(0, 1) }
            case 2 { sstore(0, 2) }
            default { sstore(0, 3) }
            result := mload(0)
        }
    }

    function set(uint256 n) internal { value = n; }

    function voidCall(uint256 n) external returns (uint256) {
        set(n);
        return value + 1;
    }

    function fail() external pure { revert(); }
}
