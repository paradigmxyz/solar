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
//@ run-call: pick true, 5, 9 => 5
//@ run-call: pick false, 5, 9 => 9
//@ run-call: pickNegated true, 5, 9 => 9
//@ run-call: pickNegated false, 5, 9 => 5
//@ run-call: store true => true
//@ run-call: store false => false
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

    // A bool parameter of a function that is also called internally reaches its body as a word, so
    // the external call and the internal call both pass the bool they hold.
    function pick(bool a, uint256 x, uint256 y) public pure returns (uint256) {
        return a ? x : y;
    }

    function pickNegated(bool a, uint256 x, uint256 y) external pure returns (uint256) {
        return pick(!a, x, y);
    }

    bool private flag;

    function store(bool a) external returns (bool) {
        flag = a;
        return flag;
    }

    function negated(bool a) external pure returns (bool) {
        return !a;
    }

    function unless(bool a, uint256 x) external pure returns (uint256, uint256) {
        require(a || x > 0, "empty");
        return (a ? 0 : 1, a ? x + 1 : x);
    }
}
