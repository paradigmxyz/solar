//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: createPair => 17, 100, 200, 4660
//@ run-call: createSingle => 5, 1, 2, 153

// A variable-length copy into low memory makes every spilled value live
// across it ride the stack. The creation payload's free-memory-pointer load
// comes after the copy in the same block; its slot is reserved before its
// definition stores it, so the copy must not reload it as a live value.
// NOTE: Unoptimized builds keep internal calls' callers off the resident
// stack layout, so they still reject values live across such a copy here.
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

contract ForwardingBufferLaterFmp {
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
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        Child token = new Child(receivers, batches, 17);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }

    function createSingle() external returns (uint256, uint256, uint256, uint256) {
        uint256[] memory batches = new uint256[](2);
        batches[0] = 1;
        batches[1] = 2;
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        Child token = new Child(one(address(0x99)), batches, 5);
        return (token.start(), token.b0(), token.b1(), uint160(token.r0()));
    }
}
