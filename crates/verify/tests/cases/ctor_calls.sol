pragma solidity ^0.8.20;
// External calls from a constructor: this contract's address holds no code until the constructor
// returns, so the callee cannot call back in and the state stays as the constructor left it.
// `ctor_calls` in cases.rs pins each constructor's verdict.
interface IHook {
    function ping() external;
    function peek() external view returns (uint256);
}
contract CtorCall {
    uint256 public x;
    /// @custom:ensures x == 7
    constructor(IHook h) { x = 7; h.ping(); }
    function set(uint256 v) external { x = v; }
    /// outside a constructor the callee can reenter `set`
    /// @custom:ensures x == 7
    function bad_call(IHook h) external { x = 7; h.ping(); }
}
contract CtorLowCall {
    uint256 public x;
    /// @custom:ensures x == 7
    constructor(address h) payable { x = 7; (bool ok, ) = h.call{value: msg.value}(""); ok; }
    function set(uint256 v) external { x = v; }
}
contract CtorStatic {
    uint256 public x;
    /// @custom:ensures x == 7
    constructor(address h) { x = 7; (bool ok, ) = h.staticcall(""); ok; }
    function set(uint256 v) external { x = v; }
}
contract CtorHelper {
    uint256 public x;
    /// an internal call made in the constructor runs there too
    /// @custom:ensures x == 7
    constructor(IHook h) { x = 7; poke(h); }
    function poke(IHook h) internal { h.ping(); }
    function set(uint256 v) external { x = v; }
}
contract CtorAsm {
    uint256 public x;
    /// @custom:ensures x == 7
    constructor(address h) {
        x = 7;
        assembly { pop(call(gas(), h, 0, 0, 0, 0, 0)) }
    }
    function set(uint256 v) external { x = v; }
}
contract CtorDelegate {
    uint256 public x;
    /// delegated code runs on this contract's storage
    /// @custom:ensures x == 7
    constructor(address h) { x = 7; (bool ok, ) = h.delegatecall(""); ok; }
    function set(uint256 v) external { x = v; }
}
contract CtorBalanceGrows {
    uint256 before;
    /// but nothing the callee does takes ETH from it
    /// @custom:ensures address(this).balance >= before
    constructor(IHook h) payable { before = address(this).balance; h.ping(); }
}
contract CtorBalance {
    /// the callee can still force ETH on this contract
    /// @custom:ensures address(this).balance == msg.value
    constructor(IHook h) payable { h.ping(); }
}
contract CtorGasOk {
    uint256 public x;
    /// a constant gas option changes nothing
    /// @custom:ensures x == 7
    constructor(address h) { x = 7; (bool ok, ) = h.call{gas: 5000}(""); ok; }
    function set(uint256 v) external { x = v; }
}
contract CtorGasWrite {
    uint256 public x;
    function g() internal returns (uint256) { x = 2; return 5000; }
    /// the gas option runs, and writes x
    /// @custom:ensures x == 7
    constructor(address h) { x = 7; (bool ok, ) = h.call{gas: g()}(""); ok; }
}
contract CtorNewBytes {
    uint256 public x;
    function size() internal returns (uint256) { x = 2; return 1; }
    /// the length of `new bytes` runs, and writes x
    /// @custom:ensures x == 7
    constructor(address h) { x = 7; (bool ok, ) = h.call(new bytes(size())); ok; }
}
contract CtorComputedTarget {
    uint256 before;
    /// the receiver's own cast does not hide the value sent
    /// @custom:ensures address(this).balance >= before
    constructor(address h) payable {
        before = address(this).balance;
        (bool ok, ) = payable(address(h)).call{value: 1}("");
        ok;
    }
}
contract CtorGasOrderOk {
    uint256 public x;
    function r(address h) internal returns (address) { x = 1; return h; }
    function g() internal returns (uint256) { x = 2; return 5000; }
    /// the receiver runs before the gas option
    /// @custom:ensures x == 2
    constructor(address h) { x = 7; (bool ok, ) = r(h).call{gas: g()}(""); ok; }
}
contract CtorGasOrderBad {
    uint256 public x;
    function r(address h) internal returns (address) { x = 1; return h; }
    function g() internal returns (uint256) { x = 2; return 5000; }
    /// the gas option runs last, so x is not 1
    /// @custom:ensures x == 1
    constructor(address h) { x = 7; (bool ok, ) = r(h).call{gas: g()}(""); ok; }
}
interface IPeek {
    function peek() external returns (uint256);
}
contract CtorRecvOrderOk {
    IPeek h;
    function g() internal returns (uint256) { h = IPeek(address(0)); return 5000; }
    /// the call goes to `a`, read before the gas option clears `h`, so `a` holds code
    /// @custom:ensures address(a) != address(0)
    constructor(IPeek a) { h = a; h.peek{gas: g()}(); }
}
contract CtorRecvOrderBad {
    IPeek h;
    function g() internal returns (uint256) { h = IPeek(address(0)); return 5000; }
    /// the gas option clears `h` after the receiver is read
    /// @custom:ensures address(h) != address(0)
    constructor(IPeek a) { h = a; h.peek{gas: g()}(); }
}
interface ITake {
    function take(uint256) external;
}
// A bound function of the same name and arity, so the verifier considers `h.take(..)` as one.
library LTake {
    function take(address a, uint256 b) internal pure returns (uint256) { return uint160(a) + b; }
}
contract CtorNestedOptsOk {
    IPeek k;
    IPeek t;
    function g() internal returns (uint256) { k = t; return 5000; }
    /// the gas option sets `k` before the argument's call reads it
    /// @custom:ensures address(k) == address(a)
    constructor(ITake h, IPeek a) { t = a; h.take{gas: g()}(k.peek()); }
}
contract CtorNestedOptsBad {
    IPeek k;
    IPeek t;
    function g() internal returns (uint256) { k = t; return 5000; }
    /// the argument's call goes to `a`, which holds code
    /// @custom:ensures address(k) == address(0)
    constructor(ITake h, IPeek a) { t = a; h.take{gas: g()}(k.peek()); }
}
