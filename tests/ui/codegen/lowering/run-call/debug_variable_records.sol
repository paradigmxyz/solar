//@ codegen-matrix: standard
//@ compile-flags: --emit=ethdebug-runtime
//@ run-call: compute(uint256,uint256) 3, 4 => 21
//@ run-call: compute(uint256,uint256) 0, 7 => 0
//@ run-call: accumulate(uint256) 5 => 10
//@ run-call: maybeBytes(bool) false => 0
//@ run-call: maybeBytes(bool) true => 2
//@ run-call: composite(uint16) 7 => 12
//@ run-call: inlined(uint256,uint256) 5, 7 => 42
//@ run-call: maybeArray(bool) false => 0
//@ run-call: maybeArray(bool) true => 2
contract DebugVariableRecords {
    struct Leaf { uint16 small; bytes payload; }

    function composite(uint16 input) external pure returns (uint256) {
        Leaf memory object = Leaf(input, bytes("abc"));
        uint256[] memory values = new uint256[](2);
        values[0] = object.small;
        values[1] = object.payload.length;
        return values[0] + values[1] + values.length;
    }

    function inlined(uint256 x, uint256 y) external pure returns (uint256) {
        unchecked { return helper(x) + helper(y); }
    }

    function helper(uint256 input) internal pure returns (uint256) {
        unchecked { uint256 local = (input * input) ^ 17; return local + 1; }
    }
    function compute(uint256 x, uint256 y) external pure returns (uint256) {
        uint256 local;
        unchecked { local = x + y; return local * x; }
    }

    function maybeBytes(bool flag_) external pure returns (uint256) {
        bytes memory buffer;
        if (flag_) buffer = bytes("hi");
        assembly { mstore(0, 123) }
        return buffer.length;
    }

    function maybeArray(bool flag_) external pure returns (uint256) {
        uint256[] memory array;
        if (flag_) array = new uint256[](2);
        assembly { mstore(0, 123) }
        return array.length;
    }

    function accumulate(uint256 n) external pure returns (uint256 total) {
        for (uint256 i; i < n; ++i) {
            total += i;
        }
    }
}
