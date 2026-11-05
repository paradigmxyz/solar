//@ codegen-matrix: standard
//@ run-call-fail: f() => Panic(0x11)
//@ run-call-fail: parenthesized() => Panic(0x11)
//@ run-call-fail: discarded() => Panic(0x11)
//@ run-call-fail: chained() => Panic(0x11)
//@ run-call-fail: divided() => Panic(0x12)
//@ run-call-fail: viaLibrary() => Panic(0x11)
//@ run-call-fail: inherited() => Panic(0x11)
//@ run-call: unchecked_() => 1
//@ run-call: folded() => 4

library L {
    bytes1 constant X = bytes1(uint8(255) + uint8(1));
}

contract Base {
    bytes2 constant Y = bytes2(uint16(65535) + uint16(1));
}

contract C is Base {
    bytes1 constant X = bytes1(uint8(255) + uint8(1));
    bytes1 constant CHAINED = X;
    uint8 constant ZERO = 0;
    bytes1 constant DIVIDED = bytes1(uint8(1) / ZERO);
    bytes4 constant FOLDED = bytes4(keccak256("folded()"));

    function f() external pure returns (uint256) {
        return X.length;
    }

    function parenthesized() external pure returns (uint256) {
        return (X).length;
    }

    function discarded() external pure returns (uint256) {
        X.length;
        return 7;
    }

    function chained() external pure returns (uint256) {
        return CHAINED.length;
    }

    function divided() external pure returns (uint256) {
        return DIVIDED.length;
    }

    function viaLibrary() external pure returns (uint256) {
        return L.X.length;
    }

    function inherited() external pure returns (uint256) {
        return Base.Y.length;
    }

    function unchecked_() external pure returns (uint256) {
        unchecked {
            return X.length;
        }
    }

    function folded() external pure returns (uint256) {
        return FOLDED.length;
    }
}
