//@ codegen-matrix: standard
//@ run-call: add => 15
//@ run-call: mixed => 3
//@ run-call: shifts => 32
//@ run-call: bits => 7
//@ run-call: cmp => true

contract C {
    function add() public pure returns (uint) {
        return 1 + 2 + 3 + 4 + 5;
    }

    function mixed() public pure returns (uint) {
        return 1 + 2 * 3 - 4;
    }

    function shifts() public pure returns (uint) {
        return 1 << 2 << 3;
    }

    function bits() public pure returns (uint) {
        return 1 | 2 | 4;
    }

    function cmp() public pure returns (bool) {
        return 1 < 2 == true;
    }
}
