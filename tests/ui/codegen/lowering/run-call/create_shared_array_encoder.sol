//@ codegen-matrix: standard
//@ run-call: createPair => 17, 100, 200, 4660
//@ run-call: createSingle => 5, 1, 2, 153

// Both creations encode `address[]` and `uint256[]` arguments, so they share
// an outlined array encoder. Its destination and returned tail stay in the
// heap, so the words copied after it are not a low-memory clobber that would
// force the caller's values through the forwarding-buffer layout.
contract Child {
    uint256 public start;
    uint256 public b0;
    uint256 public b1;
    address public r0;

    constructor(address[] memory receivers, uint256[] memory batches, uint256 startingId) {
        start = startingId;
        b0 = batches[0];
        b1 = batches[1];
        r0 = receivers[0];
    }
}

contract CreateSharedArrayEncoder {
    function one(address account) internal pure returns (address[] memory accounts) {
        accounts = new address[](1);
        accounts[0] = account;
    }

    function createPair() external returns (uint256, uint256, uint256, uint256) {
        address[] memory receivers = new address[](2);
        receivers[0] = address(0x1234);
        receivers[1] = address(0x5678);
        uint256[] memory batches = new uint256[](2);
        batches[0] = 100;
        batches[1] = 200;
        Child token = new Child(receivers, batches, 17);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }

    function createSingle() external returns (uint256, uint256, uint256, uint256) {
        uint256[] memory batches = new uint256[](2);
        batches[0] = 1;
        batches[1] = 2;
        Child token = new Child(one(address(0x99)), batches, 5);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }
}
