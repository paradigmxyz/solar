//@compile-flags: -Ztypeck
// Test: valid pure and view functions (should not produce errors)

contract C {
    uint256 public x;

    // Pure function that only does computation
    function pureAdd(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    // View function that reads state
    function viewReadsState() public view returns (uint256) {
        return x;
    }

    // View function that reads block data
    function viewReadsBlock() public view returns (uint256) {
        return block.timestamp;
    }

    // View function that reads msg.sender
    function viewReadsMsgSender() public view returns (address) {
        return msg.sender;
    }

    // Payable function that reads msg.value
    function goodPayable() public payable returns (uint256) {
        return msg.value;
    }
}
