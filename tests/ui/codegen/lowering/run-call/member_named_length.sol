//@ codegen-matrix: standard
//@ run-call: variant => 2
//@ run-call: field => 9
//@ run-call: arrayLength => 1

// A declaration may be named `length`, and then a member access resolves to
// it rather than to the builtin array member.
enum E {
    A,
    B,
    length
}

struct S {
    uint256 length;
}

contract MemberNamedLength {
    uint256[] internal values;
    S internal s;

    function variant() external pure returns (uint256) {
        return uint256(E.length);
    }

    function field() external returns (uint256) {
        s.length = 9;
        return s.length;
    }

    function arrayLength() external returns (uint256) {
        values.push(7);
        return values.length;
    }
}
