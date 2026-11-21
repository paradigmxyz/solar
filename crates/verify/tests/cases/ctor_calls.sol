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
