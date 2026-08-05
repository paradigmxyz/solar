//@ revisions: gas size
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: ModifierCalldataReturn::run(bytes) 0x0102 => 0x22ae6da6b482f9b1b19b0b897c3fd43884180a1c5ee361e1107a1bc635649dda, 7, 19

contract ModifierCalldataReturn {
    uint256 private trace;

    modifier afterReturn() {
        _;
        trace = trace * 10 + 9;
    }

    function target(bytes calldata input)
        internal
        afterReturn
        returns (bytes calldata, uint256)
    {
        trace = 1;
        return (input, 7);
    }

    function run(bytes calldata input) external returns (bytes32, uint256, uint256) {
        (bytes calldata result, uint256 value) = target(input);
        return (keccak256(result), value, trace);
    }
}
