//@ codegen-matrix: standard
//@ run-call: observeStructConstructor => 1, 1, 1
//@ run-call: observeReturnTuple => 1, 1, 1
//@ run-call: observeTupleDeclaration => 1, 1, 1
//@ run-call: observeInternalCall => 1, 1, 1
// A later operand that pushes to a storage array is seen by the copy of that array into a struct
// constructor, return tuple, tuple declaration, or internal call argument, as in solc. Reported in
// #2438, #2439, #2440, and #2441 by @0xalpharush.
contract C {
    struct S {
        uint256[] values;
        uint256 marker;
    }

    uint256[] values;

    function pushAndReturnMarker() internal returns (uint256) {
        values.push(7);
        return 1;
    }

    function pair() internal returns (uint256[] memory, uint256) {
        return (values, pushAndReturnMarker());
    }

    function consume(uint256[] memory copied, uint256 marker)
        internal
        pure
        returns (uint256, uint256)
    {
        return (copied.length, marker);
    }

    function observeStructConstructor() external returns (uint256, uint256, uint256) {
        delete values;
        S memory s = S(values, pushAndReturnMarker());
        return (s.values.length, values.length, s.marker);
    }

    function observeReturnTuple() external returns (uint256, uint256, uint256) {
        delete values;
        (uint256[] memory copied, uint256 marker) = pair();
        return (copied.length, values.length, marker);
    }

    function observeTupleDeclaration() external returns (uint256, uint256, uint256) {
        delete values;
        (uint256[] memory copied, uint256 marker) = (values, pushAndReturnMarker());
        return (copied.length, values.length, marker);
    }

    function observeInternalCall() external returns (uint256, uint256, uint256) {
        delete values;
        (uint256 copiedLength, uint256 marker) = consume(values, pushAndReturnMarker());
        return (copiedLength, values.length, marker);
    }
}
