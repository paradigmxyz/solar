//@ codegen-matrix: standard
//@ run-call: both true, true => true
//@ run-call: both true, false => false
//@ run-call: both false, true => false
//@ run-call: both false, false => false
//@ run-call: either true, true => true
//@ run-call: either true, false => true
//@ run-call: either false, true => true
//@ run-call: either false, false => false
//@ run-call: differ true, true => false
//@ run-call: differ true, false => true
//@ run-call: differ false, true => true
//@ run-call: differ false, false => false
//@ run-call: negated true => false
//@ run-call: negated false => true
//@ run-call: unless true, 7 => 0, 8
//@ run-call: unless false, 7 => 1, 7
//@ run-call-fail: 0x599b0e8800000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: 0x47a7ecc200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002
//@ run-call-fail: 0x4c75808f00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002

// Bool parameters are canonical words once the entry guard passes, so a boolean operation on
// them works on the words themselves, and a word that is not 0 or 1 still reverts.
contract BoolParamLogic {
    function both(bool a, bool b) external pure returns (bool) {
        return a && b;
    }

    function either(bool a, bool b) external pure returns (bool) {
        return a || b;
    }

    function differ(bool a, bool b) external pure returns (bool) {
        return a != b;
    }

    function negated(bool a) external pure returns (bool) {
        return !a;
    }

    function unless(bool a, uint256 x) external pure returns (uint256, uint256) {
        require(a || x > 0, "empty");
        return (a ? 0 : 1, a ? x + 1 : x);
    }
}
