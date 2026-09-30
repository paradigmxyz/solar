//@ revisions: none gas
//@[none] compile-flags: -O none -Zdump=secir
//@[gas] compile-flags: -O gas -Zdump=secir

// Covers dependencies through internal call arguments and return values, storage flows between
// entry points, externally controlled slots, controlled call targets, access control checked by
// an internal helper, and high-level call selectors and arguments. Facts are computed before
// optimization, so both revisions print the same output.

interface IERC20 {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

contract DataFlow {
    address owner;
    address payable recipient;
    address implementation;
    uint256 fee;
    IERC20 token;

    constructor(IERC20 t) {
        owner = msg.sender;
        token = t;
    }

    function setRecipient(address payable r) external {
        recipient = r;
    }

    function setFee(uint256 f) external {
        require(msg.sender == owner);
        fee = _double(f);
    }

    function setImplementation(address i) external {
        _onlyOwner();
        implementation = i;
    }

    function payout() external {
        _pay(recipient, _scaled(address(this).balance));
    }

    function upgrade(bytes calldata data) external {
        (bool ok,) = implementation.delegatecall(data);
        require(ok);
    }

    function pull(address from, uint256 amount) external {
        token.transferFrom(from, address(this), amount);
    }

    function pullSelf(uint256 amount) external {
        token.transferFrom(msg.sender, address(this), amount);
    }

    function _onlyOwner() internal view {
        require(msg.sender == owner, "owner");
    }

    function _double(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }

    function _scaled(uint256 x) internal view returns (uint256) {
        return x - fee;
    }

    function _pay(address payable to, uint256 amount) internal {
        to.transfer(amount);
    }
}
