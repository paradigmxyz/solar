//@ codegen-matrix: standard
//@ run-call: test true, false => true
//@ run-call: test false, false => true
//@ run-call: test true, true => false
//@ run-call: test false, true => true

contract DiscardedTernaryCalldataConversion {
    function choose(bool takeCalldata, uint256[][] calldata values) external pure {
        uint256[][] memory empty = new uint256[][](0);
        takeCalldata ? values : empty;
    }

    function test(bool takeCalldata, bool malformed) external returns (bool success) {
        uint256[][] memory values = new uint256[][](1);
        values[0] = new uint256[](1);
        values[0][0] = 7;
        bytes memory payload = abi.encodeWithSelector(this.choose.selector, takeCalldata, values);
        if (malformed) {
            // Replace the inner array offset with an out-of-bounds offset.
            assembly { mstore(add(payload, 0x84), not(0)) }
        }
        (success,) = address(this).call(payload);
    }
}
