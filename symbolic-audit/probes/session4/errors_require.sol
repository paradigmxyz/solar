contract ErrorsRequire {
    error Plain();
    error WithArgs(uint256 a, address b);
    error WithDyn(string s, uint256[] arr);
    error WithStruct(S s);
    struct S { uint256 a; bytes b; }
    error Narrow(uint8 a, bool b, bytes4 c);
    function requirePlain(bool c) external pure returns (uint256) { require(c); return 1; }
    function requireMsg(bool c) external pure returns (uint256) { require(c, "msg"); return 1; }
    function requireLong(bool c) external pure returns (uint256) { require(c, "a message that is definitely longer than thirty-two bytes long"); return 1; }
    function requireEmptyMsg(bool c) external pure returns (uint256) { require(c, ""); return 1; }
    function requireDynMsg(bool c, string calldata m) external pure returns (uint256) { require(c, m); return 1; }
    function requireMemMsg(bool c, string memory m) external pure returns (uint256) { require(c, m); return 1; }
    function requireCustom(bool c) external pure returns (uint256) { require(c, Plain()); return 1; }
    function requireCustomArgs(bool c, uint256 a, address b) external pure returns (uint256) { require(c, WithArgs(a, b)); return 1; }
    function requireCustomDyn(bool c, string calldata s, uint256[] calldata arr) external pure returns (uint256) { require(c, WithDyn(s, arr)); return 1; }
    function requireCustomNarrow(bool c, uint8 a, bool b, bytes4 d) external pure returns (uint256) { require(c, Narrow(a, b, d)); return 1; }
    function requireCustomEval(bool c, uint256 a) external pure returns (uint256) { require(c, WithArgs(a * 2, address(uint160(a)))); return 1; }
    function revertPlain(bool c) external pure returns (uint256) { if (c) revert(); return 1; }
    function revertMsg(bool c) external pure returns (uint256) { if (c) revert("no"); return 1; }
    function revertEmpty(bool c) external pure returns (uint256) { if (c) revert(""); return 1; }
    function revertCustom(bool c) external pure returns (uint256) { if (c) revert Plain(); return 1; }
    function revertCustomArgs(bool c, uint256 a, address b) external pure returns (uint256) { if (c) revert WithArgs(a, b); return 1; }
    function revertCustomDyn(bool c, string memory s, uint256[] memory arr) external pure returns (uint256) { if (c) revert WithDyn(s, arr); return 1; }
    function revertCustomStruct(bool c, uint256 a, bytes calldata b) external pure returns (uint256) { if (c) revert WithStruct(S(a, b)); return 1; }
    function revertCustomNarrow(uint256 raw) external pure returns (uint256) { uint8 a; bool b; bytes4 d; assembly { a := raw b := raw d := raw } revert Narrow(a, b, d); }
    function assertFalse(bool c) external pure returns (uint256) { assert(c); return 1; }
    function assertExpr(uint256 a) external pure returns (uint256) { assert(a * 2 > a); return a; }
    function panicDiv(uint256 a, uint256 b) external pure returns (uint256) { return a / b; }
    function panicMod(uint256 a, uint256 b) external pure returns (uint256) { return a % b; }
    function panicOverflow(uint256 a, uint256 b) external pure returns (uint256) { return a + b; }
    function panicUnderflow(uint256 a, uint256 b) external pure returns (uint256) { return a - b; }
    function panicMul(uint256 a, uint256 b) external pure returns (uint256) { return a * b; }
    function panicMulNarrow(uint8 a, uint8 b) external pure returns (uint8) { return a * b; }
    function panicShl(uint256 a, uint256 b) external pure returns (uint256) { return a << b; }
    function panicExp(uint256 a, uint256 b) external pure returns (uint256) { return a ** b; }
    function panicExpNarrow(uint8 a, uint8 b) external pure returns (uint8) { return a ** b; }
    function panicExpLitBase(uint256 b) external pure returns (uint256) { return 2 ** b; }
    function panicExpLitBase3(uint256 b) external pure returns (uint256) { return 3 ** b; }
    function panicExpLitBase10(uint256 b) external pure returns (uint256) { return 10 ** b; }
    function panicExpLitBase256(uint256 b) external pure returns (uint256) { return 256 ** b; }
    function panicExpLitBaseMax(uint256 b) external pure returns (uint256) { return (2**256 - 1) ** b; }
    function panicExpLitExp(uint256 a) external pure returns (uint256) { return a ** 2; }
    function panicExpLitExp3(uint256 a) external pure returns (uint256) { return a ** 3; }
    function panicExpLitExp255(uint256 a) external pure returns (uint256) { return a ** 255; }
    function panicExpU8Base(uint8 a, uint256 b) external pure returns (uint8) { return a ** b; }
    function panicExpU8Lit(uint256 b) external pure returns (uint8) { uint8 a = 2; return a ** b; }
    function panicExpU8Lit3(uint256 b) external pure returns (uint8) { uint8 a = 3; return a ** b; }
    function panicExpU8Lit16(uint256 b) external pure returns (uint8) { uint8 a = 16; return a ** b; }
    function panicExpU16(uint16 a, uint16 b) external pure returns (uint16) { return a ** b; }
    function panicExpU64(uint64 a, uint64 b) external pure returns (uint64) { return a ** b; }
    function panicExpU128(uint128 a, uint128 b) external pure returns (uint128) { return a ** b; }
    function panicExpOne(uint256 b) external pure returns (uint256) { uint256 a = 1; return a ** b; }
    function panicExpZero(uint256 b) external pure returns (uint256) { uint256 a = 0; return a ** b; }
    function panicExp0Exp(uint256 a) external pure returns (uint256) { return a ** 0; }
    function panicExp1Exp(uint256 a) external pure returns (uint256) { return a ** 1; }
    function panicArrOOB(uint256 i) external pure returns (uint256) { uint256[2] memory a; return a[i]; }
    function panicEnum(uint256 v) external pure returns (E) { return E(v); }
    enum E { A, B }
    function panicPop() external returns (uint256) { arr.pop(); return 0; }
    uint256[] arr;
    function panicAlloc(uint256 n) external pure returns (uint256) { uint256[] memory a = new uint256[](n); return a.length; }
    function panicFnPtr() external pure returns (uint256) { function() internal pure returns (uint256) f; return f(); }
    function panicNested(uint256 a, uint256 b) external pure returns (uint256) { return _inner(a, b); }
    function _inner(uint256 a, uint256 b) internal pure returns (uint256) { return a - b; }
    function requireAfterEffect(uint256 a) external returns (uint256) { arr.push(a); require(a > 5, "small"); return arr.length; }
    function errSelector() external pure returns (bytes4, bytes4, bytes4) { return (Plain.selector, WithArgs.selector, WithDyn.selector); }
    function revertEncoded(uint256 a) external pure returns (uint256) { bytes memory d = abi.encodeWithSelector(WithArgs.selector, a, address(0)); assembly { revert(add(d, 32), mload(d)) } }
    function revertEncodedErr(uint256 a) external pure returns (uint256) { bytes memory d = abi.encodeWithSelector(bytes4(keccak256("Error(string)")), "x"); assembly { revert(add(d, 32), mload(d)) } }
    function encodeError(uint256 a) external pure returns (bytes memory) { return abi.encodeWithSelector(WithArgs.selector, a, address(1)); }
    function encodeErrorCall(uint256 a) external view returns (bytes memory) { return abi.encodeCall(this.panicDiv, (a, 2)); }
    function encodeErrorSig(uint256 a) external pure returns (bytes memory) { return abi.encodeWithSignature("WithArgs(uint256,address)", a, address(1)); }
    function conditionalRevertNarrow(uint8 a) external pure returns (uint8) { if (a > 200) revert Narrow(a, true, 0x01020304); return a; }
    function requireMsgConcat(bool c, string memory s) external pure returns (uint256) { require(c, string.concat("err: ", s)); return 1; }
    function revertInLoop(uint256 n) external pure returns (uint256 s) { for (uint256 i; i < n; i++) { if (i == 3) revert WithArgs(i, address(0)); s += i; } }
    function requireTwice(uint256 a, uint256 b) external pure returns (uint256) { require(a > 0, "a"); require(b > 0, "b"); return a + b; }
    function doubleNegRequire(uint256 a) external pure returns (uint256) { require(!(a == 0), "zero"); return a; }
}
