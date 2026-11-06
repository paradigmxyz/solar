//@ codegen-matrix: standard
//@ run-call-fail: selectorReturn() => Panic(0x11)
//@ run-call-fail: selectorDiscarded() => Panic(0x11)
//@ run-call-fail: encodeCallHash() => Panic(0x11)
//@ run-call-fail: encodeCallRevert() => Panic(0x11)
//@ run-call-fail: selectorStored() => Panic(0x11)
//@ run-call-fail: selectorCompared() => Panic(0x11)
//@ run-call-fail: selectorConcat() => Panic(0x11)
//@ run-call-fail: selectorLocal() => Panic(0x11)
//@ run-call-fail: selectorConstant() => Panic(0x11)
//@ run-call-fail: selectorParenthesized() => Panic(0x11)
//@ run-call: interfaceSelector() => 0x26121ff0
//@ run-call: thisSelector() => 0xe2179b8e
//@ run-call: valueSelector() => 0x26121ff0
// Reading a constant evaluates its initializer, so `.selector` and `abi.encodeCall` of a function
// named through a constant receiver without a value revert with the initializer's panic instead of
// folding to the function's selector. A type, `this` and a constant with a value keep folding.
interface I {
    function f() external;
}

contract C {
    struct S {
        bytes4 selector;
    }

    I constant K = I(address(uint160(type(uint160).max) + uint160(1)));
    I constant OK = I(address(1));
    bytes4 constant FROM_K = K.f.selector;
    S s;

    function selectorReturn() external pure returns (uint256) {
        return uint32(K.f.selector);
    }

    function selectorDiscarded() external pure returns (uint256) {
        K.f.selector;
        return 7;
    }

    function encodeCallHash() external pure returns (uint256) {
        return uint256(keccak256(abi.encodeCall(K.f, ()))) & 1;
    }

    function encodeCallRevert() external pure {
        revert(string(abi.encodeCall(K.f, ())));
    }

    function selectorStored() external returns (uint256) {
        s = S(K.f.selector);
        return 7;
    }

    function selectorCompared() external pure returns (bool) {
        return K.f.selector == I.f.selector;
    }

    function selectorConcat() external pure returns (uint256) {
        return bytes.concat(K.f.selector).length;
    }

    function selectorLocal() external pure returns (uint256) {
        bytes4 x = K.f.selector;
        return x == bytes4(0) ? 1 : 7;
    }

    function selectorConstant() external pure returns (uint256) {
        return uint32(FROM_K);
    }

    function selectorParenthesized() external pure returns (uint256) {
        return uint32((K).f.selector);
    }

    function interfaceSelector() external pure returns (uint32) {
        return uint32(I.f.selector);
    }

    function thisSelector() external pure returns (uint32) {
        return uint32(this.g.selector);
    }

    function valueSelector() external pure returns (uint32) {
        return uint32(OK.f.selector);
    }

    function g() external {}
}
