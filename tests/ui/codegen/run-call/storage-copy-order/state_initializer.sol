//@ codegen-matrix: standard
//@ run-call: array => 3, 3, 2, 3
//@ run-call: text => 5, 5
//@ run-call: pair => 7, 2
// A state variable initialized from other storage copies the contents of the array, string, or
// struct it names, whether the source is a variable, a member, or a call result.
contract C {
    struct S {
        uint256[] values;
        uint256 marker;
    }

    uint256[] source = [1, 2, 3];
    S holder = S(source, 7);
    string note = "hello";

    uint256[] byName = source;
    uint256[] byMember = holder.values;
    uint256[] byCall = pick();
    string copiedNote = note;
    string copiedMember = member();
    S copiedHolder = holder;
    uint256[2] fixedSource = [5, 6];
    uint256[2] fixedCopy = fixedSource;

    function pick() internal view returns (uint256[] storage) {
        return source;
    }

    function member() internal view returns (string storage) {
        return note;
    }

    function array() public view returns (uint256, uint256, uint256, uint256) {
        return (byName.length, byMember.length, fixedCopy.length, byCall[2]);
    }

    function text() public view returns (uint256, uint256) {
        return (bytes(copiedNote).length, bytes(copiedMember).length);
    }

    function pair() public view returns (uint256, uint256) {
        return (copiedHolder.marker, copiedHolder.values[1]);
    }
}
