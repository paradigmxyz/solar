pragma solidity ^0.8.28;
interface IHook { function hook() external; }
interface IERC20Like { function transfer(address to, uint256 a) external returns (bool); }

uint256 constant FILE_K = 7;

library Codes {
    uint256 internal constant A = 3;
    enum Mode { Off, On }
    struct Pair { uint256 a; uint256 b; }
}

library TSlot {
    function tget(bytes32 s) internal view returns (uint256 v) { assembly { v := tload(s) } }
    function tset(bytes32 s, uint256 v) internal { assembly { tstore(s, v) } }
}

library Pay {
    function poke(IHook h) internal { h.hook(); }
}

contract Transient {
    uint256 public x;
    /// @custom:ensures result == 5
    function ok_tload() external returns (uint256) {
        assembly { tstore(1, 5) }
        uint256 v;
        assembly { v := tload(1) }
        return v;
    }
    /// transient storage is arbitrary when a call starts
    /// @custom:ensures result == 0
    function bad_tfresh() external view returns (uint256 v) { assembly { v := tload(1) } }
    /// external calls may change transient storage
    /// @custom:ensures result == 5
    function bad_thavoc(IHook h) external returns (uint256) {
        TSlot.tset(bytes32(uint256(1)), 5);
        h.hook();
        return TSlot.tget(bytes32(uint256(1)));
    }
    /// @custom:ensures result == 9
    function ok_tlib() external returns (uint256) {
        TSlot.tset(bytes32(uint256(2)), 9);
        return TSlot.tget(bytes32(uint256(2)));
    }
}

/// a transient variable's invariant must survive the end of the transaction
/// @custom:invariant t == 1
contract TransientInv {
    uint256 transient t;
    constructor() { t = 1; }
    function bad_setT() external { t = 1; }
}

/// OZ ReentrancyGuardTransient-style lock over a transient slot
/// @custom:reentrancy-guard __traw[uint256(SLOT)] != 0
contract TLock {
    bytes32 private constant SLOT = 0x9b779b17422d0df92223018b32b4d1fa46e071723d6817e2486d003becc55f00;
    uint256 public x;
    modifier nonReentrant() { _before(); _; _after(); }
    function _before() private {
        if (TSlot.tget(SLOT) != 0) revert();
        TSlot.tset(SLOT, 1);
    }
    function _after() private { TSlot.tset(SLOT, 0); }
    /// @custom:ensures x == old(x) + 1
    function ok_locked(IHook h) external nonReentrant { x += 1; h.hook(); }
    function ok_bump() external nonReentrant { x += 2; }
    /// reaches the lock slot but does not revert while it is held
    function bad_clear() external { TSlot.tset(SLOT, 0); }
}

/// without the lock, the hook can reenter and move x
contract TNoLock {
    bytes32 private constant SLOT = bytes32(uint256(7));
    uint256 public x;
    /// @custom:ensures x == old(x) + 1
    function bad_unlocked(IHook h) external { x += 1; h.hook(); }
    function bump() external { x += 2; }
}

contract Base {
    uint256 public x;
    function _set(uint256 v) internal virtual { x = v; }
}
contract Derived is Base {
    function _set(uint256 v) internal override { super._set(v + 1); }
    /// @custom:ensures x == v + 1
    function ok_super(uint256 v) external { require(v < 100); _set(v); }
    /// @custom:ensures x == v
    function bad_super(uint256 v) external { require(v < 100); _set(v); }
}

contract Consts {
    /// @custom:ensures result == 10
    function ok_consts() external pure returns (uint256) { return FILE_K + Codes.A; }
    /// @custom:ensures result == 1
    function ok_libenum() external pure returns (uint256) { return uint256(Codes.Mode.On); }
    /// @custom:ensures result == 3
    function ok_libstruct() external pure returns (uint256) { Codes.Pair memory p = Codes.Pair({a: 1, b: 2}); return p.a + p.b; }
    /// @custom:ensures result == 11
    function bad_consts() external pure returns (uint256) { return FILE_K + Codes.A; }
    /// @custom:ensures result == IHook.hook.selector
    function ok_selector() external pure returns (bytes4) { return IHook.hook.selector; }
}

contract Exprs {
    /// @custom:ensures result == n
    function ok_newarr(uint256 n) external pure returns (uint256) { uint256[] memory a = new uint256[](n); return a.length; }
    /// @custom:ensures result == 0
    function ok_newzero(uint256 n) external pure returns (uint256) { require(n > 0); uint256[] memory a = new uint256[](n); return a[0]; }
    /// @custom:ensures a < 2**160
    function ok_decode(bytes calldata d) external pure returns (address a, uint256 b) { (a, b) = abi.decode(d, (address, uint256)); }
    /// @custom:ensures result == 0
    function bad_decode(bytes calldata d) external pure returns (uint256 b) { (, b) = abi.decode(d, (address, uint256)); }
    /// @custom:ensures c ==> lo == x && hi == y
    function ok_ternTuple(bool c, uint256 x, uint256 y) external pure returns (uint256 lo, uint256 hi) { (lo, hi) = c ? (x, y) : (y, x); }
    /// @custom:ensures lo == x
    function bad_ternTuple(bool c, uint256 x, uint256 y) external pure returns (uint256 lo, uint256 hi) { (lo, hi) = c ? (x, y) : (y, x); }
    /// @custom:ensures result == 0
    function bad_slice(bytes calldata d) external pure returns (bytes20) { require(d.length >= 20); return bytes20(d[0:20]); }
    function ok_encodeCall(address t, address to, uint256 a) external returns (bool) {
        (bool ok, bytes memory res) = t.call(abi.encodeCall(IERC20Like.transfer, (to, a)));
        return ok && (res.length == 0 || abi.decode(res, (bool)));
    }
    function _two(uint256 a) internal pure returns (uint256, uint256) { return (a, a + 1); }
    /// @custom:ensures y == x + 1
    function ok_ternCall(bool c, uint256 a) external pure returns (uint256 x, uint256 y) { require(a < 100); (x, y) = c ? _two(a) : _two(a + 5); }
    /// @custom:ensures x == a
    function bad_ternCall(bool c, uint256 a) external pure returns (uint256 x, uint256 y) { require(a < 100); (x, y) = c ? _two(a) : _two(a + 5); }
    function ok_cdoffset(bytes calldata d) external pure returns (uint256 w) { assembly { w := calldataload(d.offset) } }
    /// @custom:ensures result < 256
    function ok_byteidx(bytes calldata d) external pure returns (uint256) { require(d.length > 2); return uint8(d[2]); }
    /// @custom:ensures result == 0
    function bad_byteidx(bytes calldata d) external pure returns (uint256) { require(d.length > 2); return uint8(d[2]); }
    /// @custom:ensures result < 2**32
    function ok_sig() external pure returns (uint256) { return uint32(msg.sig); }
}

contract Ptrs {
    struct C { address base; uint8 access; uint32 gen; }
    mapping(uint16 => C) internal _p;
    function _ex(uint16 i) private view returns (C storage c) {
        c = _p[i];
        if (c.base == address(0)) revert();
    }
    /// @custom:ensures _p[i].access == 1
    function ok_retptr(uint16 i) external { C storage c = _ex(i); c.access = 1; }
    /// @custom:ensures _p[i].access == old(_p[i].access)
    function bad_retptr(uint16 i) external { C storage c = _ex(i); c.access = 1; }
    /// @custom:ensures _p[i].access == 2
    function ok_lateptr(uint16 i) external { C storage c; c = _p[i]; c.access = 2; }
}

/// library code calling out still opens the contract to reentrancy
/// @custom:invariant x == y
contract LibCallout {
    uint256 public x;
    uint256 public y;
    function bad_libcall(IHook h) external { x += 1; Pay.poke(h); y += 1; }
    function ok_libcall(IHook h) external { x += 1; y += 1; Pay.poke(h); }
}

contract Asm {
    uint256 public x;
    /// the token call may reenter
    /// @custom:ensures x == old(x)
    function bad_asmcall(address token, bytes memory data) external {
        assembly { let ok := call(gas(), token, 0, add(data, 0x20), mload(data), 0, 0x20) }
    }
    /// revert bubbling ends the path
    /// @custom:ensures x == old(x)
    function ok_bubble(bool fail, bytes memory data) external {
        if (fail) { assembly { revert(add(data, 32), mload(data)) } }
    }
    /// create2 address computation over the free memory pointer
    /// @custom:ensures result < 2**160
    function ok_fmp(bytes32 salt, bytes32 h) external view returns (address addr) {
        address d = address(this);
        assembly {
            let ptr := mload(0x40)
            mstore(add(ptr, 0x40), h)
            mstore(add(ptr, 0x20), salt)
            mstore(ptr, d)
            let start := add(ptr, 0x0b)
            mstore8(start, 0xff)
            addr := and(keccak256(start, 85), 0xffffffffffffffffffffffffffffffffffffffff)
        }
    }
    /// writing behind a Solidity pointer stays unsupported (no ok_/bad_ prefix: must not verify)
    function rawWrite(bytes memory data) external pure { assembly { mstore(data, 0) } }
}


/// >> with a symbolic amount means the same in specs as in code
contract ShrSpec {
    uint256 internal constant B = 48;
    function sz(uint256 w, uint256 j) internal pure returns (uint256) { return uint32(w >> (B * j)); }
    /// @custom:ensures r == sz(w, j)
    function ok_shrSpec(uint256 w, uint256 j) external pure returns (uint256 r) { return uint32(w >> (B * j)); }
    /// @custom:ensures r == sz(w, j)
    function bad_shrSpec(uint256 w, uint256 j) external pure returns (uint256 r) { return uint32(w >> (B * j + 1)); }
}
