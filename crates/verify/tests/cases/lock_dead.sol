pragma solidity ^0.8.20;
// An entry point that the reentrancy lock leaves open, but that reverts whenever the lock-stable
// properties hold, writes nothing in a call reentered under the lock.
interface IToken {
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

/// @custom:reentrancy-guard _entered
/// @custom:lock-stable vault != address(0)
contract LockDead {
    address public immutable token;
    address public owner;
    address public vault;
    uint256 public other;
    bool private transient _entered;

    modifier nonReentrant() {
        if (_entered) revert();
        _entered = true;
        _;
        _entered = false;
    }

    constructor(address token_) {
        token = token_;
        owner = msg.sender;
    }

    // Binds once: dead while `vault` is set.
    function init(address vault_) external {
        require(msg.sender == owner);
        require(vault == address(0));
        require(vault_ != address(0));
        vault = vault_;
    }

    // Live under the lock: it writes `other` whatever `vault` is.
    function poke(uint256 v) external {
        other = v;
    }

    /// @custom:ensures msg.sender == vault
    function ok_pull(uint256 amount) external nonReentrant {
        require(msg.sender == vault && vault != address(0));
        IToken(token).transferFrom(msg.sender, address(this), amount);
    }

    // A live entry point still runs in the reentered call.
    /// @custom:ensures other == old(other)
    function bad_live(uint256 amount) external nonReentrant {
        require(msg.sender == vault && vault != address(0));
        IToken(token).transferFrom(msg.sender, address(this), amount);
    }

    // Where the lock-stable property may not hold at the call, `init` may still run.
    /// @custom:ensures vault == old(vault)
    function bad_unstable(uint256 amount) external nonReentrant {
        IToken(token).transferFrom(msg.sender, address(this), amount);
    }
}

/// @custom:reentrancy-guard _entered
/// @custom:lock-stable vault != address(0)
contract LockLive {
    address public immutable token;
    address public owner;
    address public vault;
    bool private transient _entered;

    modifier nonReentrant() {
        if (_entered) revert();
        _entered = true;
        _;
        _entered = false;
    }

    constructor(address token_) {
        token = token_;
        owner = msg.sender;
    }

    // Rebinds at any time: live under the lock.
    function setVault(address vault_) external {
        require(msg.sender == owner && vault_ != address(0));
        vault = vault_;
    }

    /// @custom:ensures msg.sender == vault
    function bad_pull(uint256 amount) external nonReentrant {
        require(msg.sender == vault && vault != address(0));
        IToken(token).transferFrom(msg.sender, address(this), amount);
    }
}
