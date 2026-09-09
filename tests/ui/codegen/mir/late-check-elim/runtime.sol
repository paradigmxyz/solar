//@ codegen-matrix: standard
//@ run-call: ascii 0 => true
//@ run-call: ascii 127 => true
//@ run-call: ascii 128 => false
//@ run-call: ascii 255 => false
//@ run-call: sum [] => 0
//@ run-call: sum [0] => 0
//@ run-call: sum [1,2,3,4,5] => 15
//@ run-call-fail: sum [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff,1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: shrinkingLoop => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: shrink 2,1 => 0
//@ run-call-fail: shrink 2,0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract LateMemoryRuntime {
    function ascii(uint8 value) external pure returns (bool) {
        bytes memory data = new bytes(1);
        data[0] = bytes1(value);
        for (uint256 i; i < data.length; ++i) {
            if (uint8(data[i]) > 127) return false;
        }
        return true;
    }

    function sum(uint256[] memory values) external pure returns (uint256 total) {
        for (uint256 i; i < values.length; ++i) total += values[i];
    }

    function shrinkingLoop() external pure returns (uint256 total) {
        bytes memory data = new bytes(1);
        for (uint256 i; i < data.length; ++i) {
            assembly { mstore(data, 0) }
            total += uint8(data[i]);
        }
    }

    function shrink(uint256 initial, uint256 length) external pure returns (uint256) {
        require(length <= initial);
        uint256[] memory values = new uint256[](initial);
        require(values.length > 0);
        assembly { mstore(values, length) }
        return values[0];
    }
}
