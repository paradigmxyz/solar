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

/// An internal call with a contract, made while the lock is held, havocs only what the callee writes
/// and what the entry points still open under the lock can write (its calls out reach only those).
/// @custom:reentrancy-guard _locked
/// @custom:invariant sC <= 10
contract LockedHelper {
    bool private _locked;
    uint256 private sB;
    uint256 private sC;
    modifier nonReentrant() { _lock(); _; _unlock(); }
    /// @custom:ensures sB == 7
    function ok_helperCallsOut(I i) external nonReentrant {
        sB = 7;
        _callOut(i);
    }
    /// @custom:ensures sB == 7
    function ok_helperCallsOutAsm(address t) external nonReentrant {
        sB = 7;
        _callOutAsm(t);
    }
    /// @custom:ensures sC <= 10
    function ok_openWriteKeepsInvariant(I i) external nonReentrant {
        _callOut(i);
    }
    /// @custom:ensures sC == old(sC)
    function bad_openWriteHavocked(I i) external nonReentrant {
        _callOut(i);
    }
    /// @custom:ensures sB == 7
    function bad_helperOwnWrite(I i) external nonReentrant {
        sB = 7;
        _callOutAndWrite(i);
    }
    /// A call out can force ETH on us (selfdestruct) even under the lock.
    /// @custom:ensures address(this).balance == old(address(this).balance)
    function bad_balanceAcrossHelper(I i) external nonReentrant {
        _callOut(i);
    }
    /// A call out can deploy or destroy code even under the lock.
    /// @custom:ensures a.code.length > 0
    function bad_codeAcrossHelper(address a, I i) external nonReentrant {
        require(a.code.length > 0);
        _callOut(i);
    }
    /// still open under the lock: its write reaches callers' calls out
    /// @custom:requires c <= 10
    function setC(uint256 c) external {
        require(c <= 10);
        sC = c;
    }
    /// @custom:requires sC <= 10
    /// @custom:ensures true
    function _callOut(I i) internal {
        i.f();
    }
    /// @custom:requires sC <= 10
    /// @custom:ensures true
    function _callOutAsm(address t) internal {
        assembly {
            pop(call(gas(), t, 0, 0, 0, 0, 0))
        }
    }
    /// @custom:requires sC <= 10
    /// @custom:ensures true
    function _callOutAndWrite(I i) internal {
        i.f();
        sB = 8;
    }
    function _lock() internal { if (_locked) revert(); _locked = true; }
    function _unlock() internal { _locked = false; }
}
