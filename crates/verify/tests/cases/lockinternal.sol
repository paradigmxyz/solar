pragma solidity ^0.8.20;
interface I { function f() external returns (uint256); }
/// @custom:ghost uint256 gB
/// @custom:reentrancy-guard _locked
contract C {
    bool private _locked;
    uint256 private sB;
    modifier nonReentrant() { _lock(); _; _unlock(); }
    /// @custom:ensures gB == 7
    function ok_ghost(I i) external nonReentrant {
        uint256 x = 7;
        /// @custom:ghost gB = x
        uint256 y = x;
        y;
        i.f();
    }
    /// @custom:ensures sB == 7
    function ok_storage(I i) external nonReentrant {
        sB = 7;
        i.f();
    }
    function view1() external view returns (address) { return address(this); }
    function _lock() internal { if (_locked) revert(); _locked = true; }
    function _unlock() internal { _locked = false; }
}
