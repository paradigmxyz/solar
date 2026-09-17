//@ codegen-matrix: standard
//@ run-call: emptyLength() => 0
//@ run-call: prefix 0x123456, 0 => 0x
//@ run-call: prefix 0x123456, 2 => 0x1234
//@ run-call: prefix 0x123456, 3 => 0x123456
//@ run-call-fail: prefix 0x123456, 1000
//@ run-call: redirected 0x123456, 0xaabbcc => 0xaabbcc

contract SliceForwarder {
    function empty() private pure returns (bytes calldata result) {
        assembly {
            result.offset := 0
            result.length := 0
        }
    }

    function emptyLength() external pure returns (uint256) {
        bytes memory result = empty();
        return result.length;
    }

    function construct(uint256 ptr, uint256 length) private pure returns (bytes calldata result) {
        assembly {
            result.offset := ptr
            result.length := length
        }
    }

    function prefix(bytes calldata input, uint256 length) external pure returns (bytes memory) {
        uint256 ptr;
        assembly { ptr := input.offset }
        return construct(ptr, length);
    }

    function redirect(bytes calldata a, bytes calldata b) private pure returns (bytes calldata) {
        assembly {
            a.offset := b.offset
            a.length := b.length
        }
        return a;
    }

    function redirected(bytes calldata a, bytes calldata b) external pure returns (bytes memory) {
        return redirect(a, b);
    }
}
