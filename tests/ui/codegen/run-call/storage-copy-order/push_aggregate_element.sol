//@ codegen-matrix: standard
//@ run-call: storageParameter => 1, 1
//@ run-call: memoryParameter => 0, 1
//@ run-call: returnValue => 0, 1
//@ run-call: returnTuple => 0, 1
//@ run-call: structConstructor => 0, 1
//@ run-call: encode => 128, 1
// `push()` on an array of arrays or structs is an operand that yields the new element's slot, which
// each consumer reads, copies, or encodes.
contract C {
    struct S {
        uint256[] values;
        uint256 marker;
    }

    uint256[][] nested;
    S[] structs;

    function fill(uint256[] storage target) internal returns (uint256) {
        target.push(7);
        return target.length;
    }

    function length(uint256[] memory copied) internal pure returns (uint256) {
        return copied.length;
    }

    function pushed() internal returns (uint256[] memory) {
        return nested.push();
    }

    function pair() internal returns (uint256[] memory, uint256) {
        return (nested.push(), 1);
    }

    function storageParameter() public returns (uint256, uint256) {
        return (fill(nested.push()), nested.length);
    }

    function memoryParameter() public returns (uint256, uint256) {
        return (length(nested.push()), nested.length);
    }

    function returnValue() public returns (uint256, uint256) {
        return (pushed().length, nested.length);
    }

    function returnTuple() public returns (uint256, uint256) {
        (uint256[] memory copied,) = pair();
        return (copied.length, nested.length);
    }

    function structConstructor() public returns (uint256, uint256) {
        S memory s = S(nested.push(), 1);
        return (s.values.length, nested.length);
    }

    function encode() public returns (uint256, uint256) {
        return (abi.encode(structs.push()).length, structs.length);
    }
}
