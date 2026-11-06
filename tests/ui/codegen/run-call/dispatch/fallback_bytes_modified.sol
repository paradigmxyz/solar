//@ codegen-matrix: standard
//@ run-call: invoke 32, 32 => 0x0000000000000000000000000000000000000000000000000000000000000020
//@ run-call: invoke 63, 1 => 0x01
//@ run-call-fail: invoke 64, 1
//@ run-call-fail: invoke 0, 65
//@ run-call-fail: invoke 0, 0x10000000000000000

contract ModifiedFallbackBytes {
    function invoke(uint256 offset, uint256 length) external returns (bytes memory) {
        (bool success, bytes memory result) = address(this).call(abi.encode(offset, length));
        require(success);
        return result;
    }

    fallback(bytes calldata input) external returns (bytes memory) {
        assembly {
            let offset := calldataload(0)
            if offset { input.offset := offset }
            input.length := calldataload(32)
        }
        return input;
    }
}
