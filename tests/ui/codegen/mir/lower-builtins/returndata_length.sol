//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: measure 0 => 64
//@ run-call: measure 3 => 96
//@ run-call: measure 33 => 128
//@ run-call: decoded 3 => 3
//@ run-call: allocatedBetween 3 => 96, 1
//@[mir] run-call: sized 256 => true

// Return data whose bytes are only measured is never copied: the length is
// `returndatasize` at the capture.
// CHECK-LABEL: fn @measure(
// CHECK-NOT: returndatacopy
// CHECK: returndatasize
// CHECK-NOT: returndatacopy
// CHECK: {{^ +}}returndata {{[0-9v]}}

// Decoding reads the bytes, so they are copied.
// CHECK-LABEL: fn @decoded(
// CHECK: returndatacopy

// A memory write between the capture and its length might reach the buffer,
// so the capture stays.
// An unoptimized build keeps every capture: its `msize` is exact, and the copy
// expands memory.
// CHECK-LABEL: fn @allocatedBetween(
// CHECK: returndatacopy
contract ReturndataLength {
    function echo(uint256 n) external pure returns (bytes memory) {
        return new bytes(n);
    }

    function measure(uint256 n) external returns (uint256) {
        (bool ok, bytes memory answer) = address(this).call(abi.encodeCall(this.echo, (n)));
        require(ok);
        return answer.length;
    }

    function decoded(uint256 n) external returns (uint256) {
        (, bytes memory answer) = address(this).call(abi.encodeCall(this.echo, (n)));
        return abi.decode(answer, (bytes)).length;
    }

    function allocatedBetween(uint256 n) external returns (uint256, uint256) {
        (, bytes memory answer) = address(this).call(abi.encodeCall(this.echo, (n)));
        uint256[] memory other = new uint256[](1);
        other[0] = 1;
        return (answer.length, other[0]);
    }

    function sized(uint256 n) external returns (bool) {
        uint256 start;
        assembly {
            start := mload(0x40)
        }
        (bool ok, bytes memory answer) = address(this).call(abi.encodeCall(this.echo, (n)));
        require(ok);
        uint256 length = answer.length;
        uint256 size;
        assembly {
            size := msize() //~[gas,size,dump] WARN: `msize` is not exact when optimizing
        }
        return size >= start + 32 + length;
    }
}
