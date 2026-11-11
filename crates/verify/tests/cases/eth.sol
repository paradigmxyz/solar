pragma solidity ^0.8.20;
interface IHook { function hook() external; }
interface IMint { function mint() external returns (uint256); }
interface IWeth { function deposit() external payable; }

/// ETH sent with call{value:} leaves the balance, even under a reentrancy lock.
/// @custom:reentrancy-guard locked
/// @custom:invariant address(this).balance >= owed
contract Escrow {
    uint256 owed;
    bool locked;
    modifier lock() { require(!locked); locked = true; _; locked = false; }
    function ok_deposit() external payable lock { owed += msg.value; }
    function ok_pay(address to, uint256 a) external lock {
        require(a <= owed);
        owed -= a;
        (bool s, ) = to.call{value: a}("");
        require(s);
    }
    function ok_wrap(IWeth w, uint256 a) external lock {
        require(a <= owed);
        owed -= a;
        w.deposit{value: a}();
    }
}
/// @custom:reentrancy-guard locked
/// @custom:invariant address(this).balance >= owed
contract EscrowLeak {
    uint256 owed;
    bool locked;
    modifier lock() { require(!locked); locked = true; _; locked = false; }
    function bad_payWithoutDebit(address to, uint256 a) external lock {
        (bool s, ) = to.call{value: a}("");
        require(s);
    }
    function bad_wrapWithoutDebit(IWeth w, uint256 a) external lock { w.deposit{value: a}(); }
}
/// Invariants describe the state before msg.value is credited.
/// @custom:invariant address(this).balance == total
contract Exact {
    uint256 total;
    function ok_deposit() external payable { total += msg.value; }
    function bad_skim() external payable { total += msg.value + 1; }
}
/// try/catch: the callee either returns or reverts (undoing its effects) and a catch clause runs.
/// @custom:reentrancy-guard locked
contract Minter {
    uint256 public n;
    bool public paused;
    bool locked;
    IMint m;
    modifier lock() { require(!locked); locked = true; _; locked = false; }
    /// @custom:ensures paused || n == old(n) + 1
    function ok_try() external lock {
        try m.mint() returns (uint256) { n += 1; } catch Error(string memory) { paused = true; }
    }
    /// @custom:ensures n == old(n) + 1
    function bad_catchIgnored() external lock {
        try m.mint() returns (uint256) { n += 1; } catch { }
    }
}
/// __time: the latest block.timestamp seen; block time never goes backwards.
/// @custom:invariant last <= __time
contract Clock {
    uint256 public last;
    function ok_stamp() external { last = block.timestamp; }
    function ok_monotone() external view { assert(block.timestamp >= last); }
    function bad_future() external { last = block.timestamp + 1; }
}

interface ICode { function poke() external; function peek() external view returns (uint256); }
/// Code read in a spec is the code the body read, until outside code runs.
contract CodeEpoch {
    /// @custom:ensures a.code.length > 0
    function ok_codeKeptOverViewCall(ICode a) external view {
        require(a.code.length > 0);
        a.peek();
    }
    /// @custom:ensures old(a.code.length) > 0
    function ok_oldCodeBeforeCall(ICode a) external {
        require(a.code.length > 0);
        a.poke();
    }
    /// @custom:ensures a.code.length > 0
    function bad_codeChangesAcrossCall(ICode a) external {
        require(a.code.length > 0);
        a.poke();
    }
}
