//@ compile-flags: -Zdump=secir

// Covers nested mappings, dynamic arrays, packed struct fields, modifiers with custom errors,
// `tx.origin`, delegate and static calls, contract creation, inline assembly, and selfdestruct.

interface IToken {
    function transfer(address to, uint256 value) external returns (bool);
}

contract Child {}

contract CallsAndStorage {
    error NotOwner();

    struct Pair {
        uint128 a;
        uint128 b;
    }

    address owner;
    mapping(address => mapping(address => uint256)) allowance;
    uint256[] items;
    Pair pair;

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    function pay(IToken token, address to, uint256 value) external onlyOwner {
        allowance[msg.sender][to] = value;
        require(token.transfer(to, value));
        items.push(value);
    }

    function exec(address target, bytes calldata data) external {
        require(tx.origin == owner);
        (bool ok,) = target.delegatecall(data);
        if (!ok) revert();
        pair.b = uint128(block.timestamp);
    }

    function peek(address target) external view returns (uint256 x) {
        (, bytes memory result) = target.staticcall("");
        x = result.length + pair.a + items[x];
    }

    function spawn() external onlyOwner returns (address) {
        return address(new Child());
    }

    function raw(uint256 x) external {
        assembly {
            sstore(add(x, 1), mul(x, 2))
        }
    }

    function kill() external onlyOwner {
        selfdestruct(payable(msg.sender));
    }
}
