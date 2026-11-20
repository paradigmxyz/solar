pragma solidity ^0.8.20;
interface IHook { function hook() external; function peek() external view returns (uint256); }
/// @custom:reentrancy-guard locked
contract Guarded {
    uint256 public x;
    uint256 public y;
    bool locked;
    modifier lock() { require(!locked); locked = true; _; locked = false; }
    /// @custom:ensures x == old(x) + 1
    function ok_guarded(IHook h) external lock { x += 1; h.hook(); }
    /// callable while locked, but writes only y
    /// @custom:ensures y == old(y) + 1
    function bad_unguarded(IHook h) external { y += 1; h.hook(); }
}
/// An unguarded entry point that writes x can run during the guarded call's hook.
/// @custom:reentrancy-guard locked
contract GuardedOpenWriter {
    uint256 public x;
    bool locked;
    modifier lock() { require(!locked); locked = true; _; locked = false; }
    /// @custom:ensures x == old(x) + 1
    function bad_openWriter(IHook h) external lock { x += 1; h.hook(); }
    function setX(uint256 v) external { x = v; }
}
/// @custom:reentrancy-guard locked
/// @custom:lock-stable !frozen
contract Stable {
    bool public frozen;
    uint256 public x;
    bool locked;
    modifier lock() { require(!locked); locked = true; _; locked = false; }
    /// @custom:ensures !frozen
    function ok_stable(IHook h) external lock { require(!frozen); x += 1; h.hook(); }
    function ok_bump() external { x += 2; }
}
/// @custom:reentrancy-guard locked
/// @custom:lock-stable !frozen
contract Unstable {
    bool public frozen;
    bool locked;
    function bad_freeze() external { frozen = true; }
}
/// @custom:reentrancy-guard locked
contract LockReleaser {
    bool locked;
    function bad_unlock() external { locked = false; }
}
contract Unguarded {
    uint256 public x;
    /// @custom:ensures x == old(x)
    function bad_havoc(IHook h) external { h.hook(); }
    /// @custom:ensures x == old(x)
    function ok_static(IHook h) external view returns (uint256) { return h.peek(); }
    /// @custom:ensures x == old(x)
    function ok_transfer(address payable to) external { to.transfer(1); }
    /// @custom:ensures x == old(x)
    function bad_lowlevel(address to) external { (bool ok, ) = to.call(""); require(ok); }
}
