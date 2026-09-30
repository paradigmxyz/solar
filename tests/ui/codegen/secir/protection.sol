//@ compile-flags: -Zdump=secir

// Covers entry checks and guarded effects: modifiers, checks made by internal helpers (including a
// role lookup keyed by `msg.sender`), checks on a local copy of `msg.sender`, checks that only
// dominate one branch, early returns that are not checks, unguarded `selfdestruct` and
// `delegatecall`, and argument validation.

contract Protection {
    address owner;
    mapping(address => bool) admins;
    uint256 value;
    mapping(bytes32 => mapping(address => bool)) roles;

    bytes32 constant MINTER = keccak256("MINTER");

    modifier onlyOwner() {
        require(msg.sender == owner, "owner");
        _;
    }

    modifier onlyAdmin() {
        _checkAdmin();
        _;
    }

    function _checkAdmin() internal view {
        if (!admins[msg.sender]) revert();
    }

    modifier onlyRole(bytes32 role) {
        _checkRole(role, msg.sender);
        _;
    }

    function _checkRole(bytes32 role, address account) internal view {
        if (!hasRole(role, account)) revert();
    }

    function hasRole(bytes32 role, address account) public view returns (bool) {
        return roles[role][account];
    }

    function mint(uint256 v) external onlyRole(MINTER) {
        value += v;
    }

    function setValue(uint256 v) external onlyOwner {
        value = v;
    }

    function setByAdmin(uint256 v) external onlyAdmin {
        value = v;
    }

    function setWithLocalCopy(uint256 v) external {
        address sender = msg.sender;
        require(sender == owner);
        value = v;
    }

    function oneBranch(uint256 v) external {
        if (v > 10) {
            require(msg.sender == owner);
            value = v;
        }
        value += 1;
    }

    function earlyReturn(uint256 v) external {
        if (msg.sender != owner) return;
        value = v;
    }

    function kill() external {
        selfdestruct(payable(msg.sender));
    }

    function forward(address target, bytes calldata data) external payable {
        (bool ok,) = target.delegatecall(data);
        require(ok);
    }

    function claimOwnership() external {
        owner = msg.sender;
    }

    function setOwner(address a) external onlyOwner {
        require(a != address(0));
        owner = a;
    }
}
