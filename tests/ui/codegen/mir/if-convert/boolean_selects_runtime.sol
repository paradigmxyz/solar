//@ codegen-matrix: standard
//@ run-call: either 1 => true
//@ run-call: either 2 => true
//@ run-call: either 3 => false
//@ run-call: any 0x01ffc9a7 => true
//@ run-call: any 0x80ac58cd => true
//@ run-call: any 0x5b5e139f => true
//@ run-call: any 0x12345678 => false
//@ run-call: within 4 => true
//@ run-call: within 3 => false
//@ run-call: within 10 => false
//@ run-call: outside 10 => true
//@ run-call: outside 4 => false
//@ run-call: outside 2 => true

contract BooleanSelects {
    function either(uint256 a) external pure returns (bool) {
        return a == 1 || a == 2;
    }

    function any(bytes4 id) external pure returns (bool) {
        return id == 0x01ffc9a7 || id == 0x80ac58cd || id == 0x5b5e139f;
    }

    function within(uint256 a) external pure returns (bool) {
        return a < 10 && a > 3;
    }

    function outside(uint256 a) external pure returns (bool) {
        return a < 10 ? a < 4 : true;
    }
}
