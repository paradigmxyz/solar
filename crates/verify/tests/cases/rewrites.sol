pragma solidity ^0.8.20;
interface I { function f() external returns (uint256); function g(uint256 a) external returns (bytes4); }
library Flags {
    uint8 internal constant QUOTE = 1;
    uint8 internal constant BEFORE = 2;
    uint8 internal constant AFTER = 4;
    uint8 internal constant ALL = QUOTE | BEFORE | AFTER;
}
library M {
    enum Rounding { Floor, Ceil }
    function div(uint256 a, uint256 b, Rounding r) internal pure returns (uint256) {
        uint256 q = a / b;
        if (r == Rounding.Ceil && q * b != a) q += 1;
        return q;
    }
}
contract Base {
    uint256 internal x;
    function _set(uint256 v) internal virtual { x = v; }
    function _msgData() internal view virtual returns (bytes calldata) { return msg.data; }
}
contract Mid is Base {
    uint256 internal pend;
    function _set(uint256 v) internal virtual override { pend = 0; super._set(v); }
}
/// @custom:reentrancy-guard locked == 2
contract C is Mid {
    struct S { uint256 a; bool b; address c; }
    struct T { S s; uint256 n; }
    uint256 private locked = 1;
    uint256 internal y;
    modifier nonReentrant() { if (locked != 1) revert(); locked = 2; _; locked = 1; }

    /// @custom:ensures x == v && pend == 0
    function ok_super(uint256 v) external { _set(v); }
    /// @custom:ensures x == v + 1
    function bad_super(uint256 v) external { _set(v); }

    /// @custom:ensures n == msg.data.length
    function ok_msgdata() external view returns (uint256 n) { n = _msgData().length; }

    /// @custom:ensures y == 5
    function ok_uintlock(I i) external nonReentrant { y = 5; i.f(); }
    /// @custom:ensures y == 6
    function bad_uintlock(I i) external nonReentrant { y = 5; i.f(); }

    /// @custom:ensures result == (s.b ? s.a : 0)
    function ok_decode_struct(bytes calldata d) external pure returns (uint256 result) {
        S memory s = abi.decode(d, (S));
        result = s.b ? s.a : 0;
    }
    /// @custom:ensures result == 1
    function bad_decode_struct(bytes calldata d) external pure returns (uint256 result) {
        S memory s = abi.decode(d, (S));
        result = s.b ? s.a : 0;
    }
    /// @custom:ensures result == t.n
    function ok_decode_nested(bytes calldata d) external pure returns (uint256 result) {
        T memory t = abi.decode(d, (T));
        result = t.n;
        t.s.a;
    }
    /// @custom:ensures d.length == 0 || r
    function ok_decode_bool(bytes calldata d) external pure returns (bool r) {
        if (d.length != 0 && !abi.decode(d, (bool))) revert();
        r = true;
    }
    /// @custom:ensures r
    function bad_decode_bool(bytes calldata d) external pure returns (bool r) {
        r = d.length == 0 || abi.decode(d, (bool));
    }

    /// @custom:ensures result == 5
    function ok_libconst() external pure returns (uint8 result) { result = Flags.QUOTE | Flags.AFTER; }
    /// @custom:ensures result == 4
    function bad_libconst() external pure returns (uint8 result) { result = Flags.QUOTE | Flags.AFTER; }

    /// @custom:ensures result == ((f & ~uint8(7)) == 0)
    function ok_libconst_all(uint8 f) external pure returns (bool result) { result = (f & ~Flags.ALL) == 0; }
    /// @custom:ensures result == ((f & ~uint8(3)) == 0)
    function bad_libconst_all(uint8 f) external pure returns (bool result) { result = (f & ~Flags.ALL) == 0; }

    /// @custom:ensures result == 4
    function ok_enumpath() external pure returns (uint256 result) { result = M.div(7, 2, M.Rounding.Ceil); }
    /// @custom:ensures result == 3
    function bad_enumpath() external pure returns (uint256 result) { result = M.div(7, 2, M.Rounding.Ceil); }

    /// @custom:ensures result == n
    function ok_newstructarr(uint256 n) external pure returns (uint256 result) {
        S[] memory a = new S[](n);
        result = a.length;
    }
    /// @custom:requires n > 0
    /// @custom:ensures result == 0
    function ok_newarr(uint256 n) external pure returns (bytes32 result) {
        bytes32[] memory a = new bytes32[](n);
        result = a[0];
    }
    /// @custom:requires n > 0
    /// @custom:ensures result == 1
    function bad_newarr(uint256 n) external pure returns (bytes32 result) {
        bytes32[] memory a = new bytes32[](n);
        result = a[0];
    }

    /// @custom:ensures true
    function ok_trailing_assert(uint256 v) external pure {
        if (v > 3) {
            v = 2;
            /// @custom:assert v == 2
        }
        /// @custom:assert v <= 3 || v == 2
    }
    function bad_trailing_assert(uint256 v) external pure {
        if (v > 3) {
            v = 1;
            /// @custom:assert v == 2
        }
    }
    function bad_trailing_assert_outer(uint256 v) external pure {
        if (v > 3) {
            v = 1;
        }
        /// @custom:assert v == 1
    }
    // expect: is attached to no statement
    function bad_trailing_ghost(uint256 v) external pure {
        v = 1;
        /// @custom:ghost v = 2
    }

    /// @custom:ensures y == 5
    function ok_ternary_lock(I i, bool c) external nonReentrant {
        y = 5;
        uint256 z = c ? i.f() : 0;
        z;
    }

    /// @custom:ensures result == I.g.selector
    function ok_selector() external pure returns (bytes4 result) { result = I.g.selector; }
    /// @custom:ensures result == bytes4(0)
    function bad_selector() external pure returns (bytes4 result) { result = I.g.selector; }
    /// @custom:ensures true
    function ok_encodecall(uint256 v) external pure returns (bytes memory result) { result = abi.encodeCall(I.g, (v)); }

    /// @custom:ensures result == v
    function ok_newstructarr_set(uint256 v) external pure returns (uint256 result) {
        S[] memory a = new S[](2);
        S memory s;
        s.a = v;
        a[1] = s;
        result = a[1].a;
    }
    /// @custom:ensures result == v + 1
    function bad_newstructarr_set(uint256 v) external pure returns (uint256 result) {
        S[] memory a = new S[](2);
        S memory s;
        s.a = v;
        a[1] = s;
        result = a[1].a;
    }
}

contract D {
    enum Op { A, B, C }
    struct Enc { bytes32 k; bytes ct; }
    struct ZR { uint256 keyIndex; Enc encrypted; address refund; }
    struct CB { Op op; uint128 minA; bytes32 id; ZR zoneReturn; }
    struct SD { Op op; address spender; uint128 maxInput; bytes spenderData; ZR zoneReturn; }
    struct Send { address to; address token; uint128 amount; address vault; bytes32 ref; }
    uint256 public last;

    /// @custom:ensures last == s.amount
    function _use(Send memory s) private { last = s.amount; }

    /// @custom:ensures last <= type(uint128).max
    function ok_decode_arg(bytes calldata data) external { _use(abi.decode(data, (Send))); }
    /// @custom:ensures last == 0
    function bad_decode_arg(bytes calldata data) external { _use(abi.decode(data, (Send))); }

    /// @custom:ensures result == data.zoneReturn.keyIndex || data.op != Op.A
    function ok_decode_nested2(bytes calldata callbackData) external pure returns (uint256 result) {
        CB memory data = abi.decode(callbackData, (CB));
        if (data.op == Op.A) result = data.zoneReturn.keyIndex;
        address r = data.zoneReturn.refund;
        r;
    }
    /// @custom:ensures result == 1
    function bad_decode_nested2(bytes calldata callbackData) external pure returns (uint256 result) {
        CB memory data = abi.decode(callbackData, (CB));
        result = data.zoneReturn.keyIndex;
    }
    /// @custom:ensures n >= s.spenderData.length
    function ok_decode_bytesfield(bytes calldata callbackData) external pure returns (uint256 n) {
        SD memory s = abi.decode(callbackData, (SD));
        n = s.spenderData.length + uint256(s.maxInput);
    }
    /// @custom:ensures ok
    function ok_decode_ret(address token, bytes memory data) external returns (bool ok) {
        (bool s, bytes memory returnData) = token.call(data);
        if (!s) revert();
        if (returnData.length != 0 && !abi.decode(returnData, (bool))) revert();
        ok = true;
    }
    /// decoding is a function of the bytes: a spec can name what the code decoded
    /// @custom:ensures returnData.length == 0 || abi.decode(returnData, (bool))
    function ok_decode_same(address token, bytes memory data) external {
        (bool s, bytes memory returnData) = token.call(data);
        if (!s || (returnData.length != 0 && !abi.decode(returnData, (bool)))) revert();
    }
    /// @custom:ensures false
    function bad_decode_ret(address token, bytes memory data) external {
        (bool s, bytes memory returnData) = token.call(data);
        if (!s || (returnData.length != 0 && !abi.decode(returnData, (bool)))) revert();
    }
    /// different bytes may decode differently
    /// @custom:ensures abi.decode(a, (uint8)) == abi.decode(b, (uint8))
    function bad_decode_diff(bytes memory a, bytes memory b) external pure {}
    /// @custom:ensures result <= 255
    function ok_decode_range(bytes memory b) external pure returns (uint256 result) { result = abi.decode(b, (uint8)); }
}

contract H {
    event E(bytes32 h);
    /// abi.encode of dynamic arrays hashes to an arbitrary value (no z3 sort error)
    /// @custom:ensures true
    function ok_hash_arrays(uint64 v, address[] calldata r, uint256[] calldata a) external {
        bytes32 h = keccak256(abi.encode(v, r, a));
        emit E(h);
    }
    /// Each hash of a dynamic array is arbitrary, so this must not verify. z3 finds no model of
    /// the hash axioms, so the verdict is unknown rather than a counterexample.
    // expect: solver returned unknown
    /// @custom:ensures result == keccak256(abi.encode(v, r))
    function bad_hash_arrays(uint64 v, address[] calldata r) external pure returns (bytes32 result) {
        result = keccak256(abi.encode(v, r));
    }
    /// @custom:requires n > 0
    /// @custom:ensures result == keccak256(abi.encode(r[0]))
    function ok_leaves(address[] calldata r, uint256 n) external pure returns (bytes32 result) {
        require(r.length == n);
        bytes32[] memory leaves = new bytes32[](n);
        for (uint256 i; i < n; ++i) {
            leaves[i] = keccak256(abi.encode(r[i]));
        }
        result = keccak256(abi.encode(r[0]));
        leaves;
    }
}

contract Tern {
    uint256 public y;
    function _set() internal returns (uint256) { y = 1; return 1; }
    /// a call in one branch of a conditional does not reach the paths through the other
    /// @custom:ensures y == 1
    function bad_ternary_effect(bool c) external returns (uint256 z) { y = 0; z = c ? _set() : 0; }
    /// @custom:ensures y == (c ? 1 : 0) && z == y
    function ok_ternary_effect(bool c) external returns (uint256 z) { y = 0; z = c ? _set() : 0; }
}
