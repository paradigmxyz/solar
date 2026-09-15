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
//@ run-call: echo 42 => 42
//@ run-call: transient 99 => 99
//@ run-call: environment => true
//@ run-call: copies 123 => 123
//@ run-call: tuple 20 => 41
//@ run-call: frame 5 => 11
//@ run-call: terminalCall 42 => 42
//@ run-call: usedCall 42 => 44
//@ run-call-fail: fail

//@ run-call: Data::literal => 42
//@ run-call: Data::spawn => 42

//@ run-call: Constructor::read; constructor=[42] => 42

//@ run-call: Immutables::read; constructor=[42] => 42

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

    function set(uint256 n) internal returns (uint256) { value = n; return n + 2; }

    function usedCall(uint256 n) external returns (uint256) { return set(n); }

    function voidCall(uint256 n) external returns (uint256) {
        set(n);
        return value + 1;
    }

    function echo(uint256 value) external view returns (uint256 result) {
        assembly {
            mstore(0, value)
            if iszero(staticcall(gas(), 4, 0, 32, 32, 32)) { revert(0, 0) }
            if iszero(eq(returndatasize(), 32)) { revert(0, 0) }
            returndatacopy(0, 0, 32)
            result := mload(0)
        }
    }

    function transient(uint256 input) external returns (uint256 result) {
        assembly {
            tstore(0, input)
            result := tload(0)
            log1(0, 0, result)
        }
    }

    function environment() external view returns (bool result) {
        assembly { result := and(gt(chainid(), 0), gt(extcodesize(address()), 0)) }
    }

    function copies(uint256 input) external pure returns (uint256 result) {
        assembly {
            calldatacopy(0, 4, 32)
            mcopy(32, 0, 32)
            result := mload(32)
        }
    }

    function pair(uint256 n) internal pure returns (uint256, uint256) { return (n, n + 1); }

    function tuple(uint256 n) external pure returns (uint256) {
        (uint256 a, uint256 b) = pair(n);
        return a + b;
    }

    function sum(uint256 n) internal pure returns (uint256) {
        uint256[2] memory words = [n, n + 1];
        return words[0] + words[1];
    }

    function frame(uint256 n) external pure returns (uint256) { return sum(n); }

    function terminal(uint256 n) internal pure returns (uint256) {
        assembly { mstore(0, n) return(0, 32) }
    }

    function terminalCall(uint256 n) external pure returns (uint256) {
        terminal(n);
        return 0;
    }

    function fail() external pure { revert(); }
}

contract Child {
    function read() external pure returns (uint256) { return 42; }
}

contract Data {
    function spawn() external returns (uint256) { return (new Child()).read(); }

    function literal() external pure returns (uint256 value) {
        bytes memory data = hex"2aabababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababababab";
        assembly { value := byte(0, mload(add(data, 32))) }
    }
}

contract Constructor {
    uint256 number;
    constructor(uint256 n) { number = n; }
    function read() external view returns (uint256) { return number; }
}

contract Immutables {
    uint256 immutable number;
    constructor(uint256 n) { number = n; }
    function read() external view returns (uint256) { return number; }
}
