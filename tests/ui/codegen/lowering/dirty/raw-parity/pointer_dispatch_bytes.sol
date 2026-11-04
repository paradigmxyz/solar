//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::retyped => 1
//@ run-call: U::retyped => 1
//@ run-call: V::retyped => 1

// A function pointer that assembly retyped passes a `bytes4` through a full word, to a `bytes4`
// or to a user-defined value type over it, in either direction. A parameter that is not a raw
// carrier still sees a clean value, as in solc via-IR.

contract C {
    function f(bytes4 x) internal pure returns (uint256) {
        return x == bytes4(0x11111111) ? 1 : 2;
    }
    function retyped() external pure returns (uint256 r) {
        function(bytes4) internal pure returns (uint256) p = f;
        function(uint256) internal pure returns (uint256) q;
        assembly { q := p }
        r = q((0x11111111 << 224) | 1);
    }
}

type B is bytes4;

contract U {
    function f(B x) internal pure returns (uint256) {
        return B.unwrap(x) == bytes4(0x11111111) ? 1 : 2;
    }
    function retyped() external pure returns (uint256 r) {
        function(B) internal pure returns (uint256) p = f;
        function(bytes4) internal pure returns (uint256) q;
        assembly { q := p }
        bytes4 a;
        assembly { a := or(shl(224, 0x11111111), 1) }
        r = q(a);
    }
}

contract V {
    function f(bytes4 x) internal pure returns (uint256) {
        return x == bytes4(0x11111111) ? 1 : 2;
    }
    function retyped() external pure returns (uint256 r) {
        function(bytes4) internal pure returns (uint256) p = f;
        function(B) internal pure returns (uint256) q;
        assembly { q := p }
        B a;
        assembly { a := or(shl(224, 0x11111111), 1) }
        r = q(a);
    }
}
