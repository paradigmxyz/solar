//@ codegen-matrix: standard
//@ run-call: identity 7 => 15
//@ run-call: identity 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: power 12345 => 1881365963625, 286718338524635465625
//@ run-call: hashes 1 => 0xdafcff9f8b8a4b1ea50d9370200825095d27b9ebc1183f51cd81133e187ec11e, 0x915c939ec95e0e5fd3ce2a1f2da18b8abefc846ac943a4e1b58c5918e3545d19
//@ run-call: returned 5 => 0x0000000000000000000000000000000000000000000000000000000000000005
//@ run-call: stored 6 => 0x0000000000000000000000000000000000000000000000000000000000000006
//@ run-call: passed 9 => 9

// An internal helper that encodes a temporary, calls a precompile with it, and returns a scalar
// leaves nothing of its objects to read after `ret`, so the stores of their length words go. A
// temporary whose pointer the helper returns, stores into another object, or passes to a callee
// stays exact.
contract DeadAllocationReturn {
    uint256 constant M = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F;

    function callIdentity(uint256 x) internal view returns (uint256 r) {
        (bool ok, bytes memory out) = address(4).staticcall(abi.encode(x));
        require(ok && out.length == 32);
        assembly ("memory-safe") {
            r := mload(add(out, 32))
        }
    }

    function identity(uint256 x) external view returns (uint256) {
        unchecked {
            return callIdentity(x) + callIdentity(x + 1);
        }
    }

    function modexp(uint256 b, uint256 e) internal view returns (uint256) {
        (bool ok, bytes memory out) = address(5).staticcall(abi.encode(32, 32, 32, b, e, M));
        require(ok && out.length == 32);
        return abi.decode(out, (uint256));
    }

    function power(uint256 b) external view returns (uint256, uint256) {
        return (modexp(b, 3), modexp(b, 5));
    }

    function digest(uint256 a, bytes memory b) internal pure returns (bytes32) {
        return keccak256(abi.encode(a, b));
    }

    function hashes(uint256 a) external pure returns (bytes32, bytes32) {
        bytes memory m = abi.encodePacked(a);
        return (digest(a, m), digest(a + 1, m));
    }

    function build(uint256 x) internal pure returns (bytes memory) {
        return abi.encode(x);
    }

    function returned(uint256 x) external pure returns (bytes memory) {
        return build(x);
    }

    function stored(uint256 x) external pure returns (bytes memory) {
        bytes[] memory all = new bytes[](1);
        all[0] = abi.encode(x);
        return all[0];
    }

    function length(bytes memory b) internal pure returns (uint256) {
        return b.length;
    }

    function passed(uint256 x) external pure returns (uint256) {
        return length(abi.encode(x, x)) / 64 * x;
    }
}
