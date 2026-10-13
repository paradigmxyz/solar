//@ codegen-matrix: standard
//@ run-call: C::x; constructor=[[1, 2, 3], 4] => 4

// A scalar behind the largest static memory array loads its word directly
// instead of taking a lazy argument for every head word before it.
contract Huge {
    uint256 public x;

    constructor(uint256[134217727] memory a, uint256 i) {
        x = a[i];
    }
}

contract C {
    uint256 public x;

    constructor(uint256[3] memory a, uint256 i) {
        x = a[2] + i - 3;
    }
}
