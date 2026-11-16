//@ run-call: fromCall => 0x1200000000000000000000000000000000000000000000000000000000000000, 123
//@ run-call: fromConditional false => 9
//@ run-call: fromConditional true => 7

contract NestedMultiReturnAssignment {
    uint256 private trace;
    bytes32[1] private words;

    function fromCall() external returns (bytes32, uint256) {
        (((words[index(3)],),)) = (pair(), ignored());
        return (words[0], trace);
    }

    function fromConditional(bool flag) external pure returns (uint256) {
        uint256 value;
        (((value,),)) = (flag ? (7, 8) : (9, 10), 11);
        return value;
    }

    function pair() internal returns (bytes1, uint256) {
        trace = trace * 10 + 1;
        return (0x12, 7);
    }

    function ignored() internal returns (uint256) {
        trace = trace * 10 + 2;
        return 99;
    }

    function index(uint256 tag) internal returns (uint256) {
        trace = trace * 10 + tag;
        return 0;
    }
}
