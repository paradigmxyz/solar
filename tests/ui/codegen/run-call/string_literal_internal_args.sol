//@ run-call: libLiteral() => 3
//@ run-call: libConstant() => 20
//@ run-call: libHex() => 2
//@ run-call: libLong() => 40
//@ run-call: literalContent() => 0x6869000000000000000000000000000000000000000000000000000000000000
//@ run-call: encodeConstant() => 132
//@ run-call: encodeConstantCast() => 100

// A string/bytes literal or a `constant` string reference passed to an
// internal function lowers to its content word unless it is materialized as a
// `[length][data...]` memory object; the callee then dereferences the content
// as a pointer. Regression tests for the internal-call argument and ABI-encode
// paths.

library Errors {
    string internal constant MAX_UINT128_EXCEEDED = "max uint128 exceeded";
}

library L {
    function lenPlus(string memory s, uint256 x) internal pure returns (uint256) {
        return bytes(s).length + x;
    }

    function lenBytes(bytes memory b) internal pure returns (uint256) {
        return b.length;
    }

    function firstWord(string memory s) internal pure returns (bytes32 w) {
        assembly {
            w := mload(add(s, 32))
        }
    }
}

contract StringLiteralInternalArgs {
    function libLiteral() external pure returns (uint256) {
        return L.lenPlus("hi", 1);
    }

    function libConstant() external pure returns (uint256) {
        return L.lenPlus(Errors.MAX_UINT128_EXCEEDED, 0);
    }

    function libHex() external pure returns (uint256) {
        return L.lenBytes(hex"aabb");
    }

    function libLong() external pure returns (uint256) {
        return L.lenPlus("0123456789012345678901234567890123456789", 0);
    }

    function literalContent() external pure returns (bytes32) {
        return L.firstWord("hi");
    }

    function encodeConstant() external pure returns (uint256) {
        bytes memory payload =
            abi.encodeWithSignature("log(string,uint256)", Errors.MAX_UINT128_EXCEEDED, uint256(1));
        return payload.length;
    }

    function encodeConstantCast() external pure returns (uint256) {
        bytes memory payload =
            abi.encodeWithSignature("f(bytes)", bytes(Errors.MAX_UINT128_EXCEEDED));
        return payload.length;
    }
}
