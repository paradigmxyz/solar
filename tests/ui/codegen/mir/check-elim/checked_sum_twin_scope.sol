//@ codegen-matrix: standard
//@ run-call: f 1, 2, false => 1
//@ run-call: f 1, 2, true => 8
//@ run-call: f 1, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, false => 0
//@ run-call-fail: f 1, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, true => Panic(17)
//@ run-call: f 5, 1, true => 0
// A checked sum equals the wrapping sum of the same operands only after its check passed. The
// unchecked `y + 5` runs before the checked one, which only one path reaches, so it may wrap.
contract Twins {
    function f(uint256 x, uint256 y, bool b) public pure returns (uint256 r) {
        if (y < x) return 0;
        unchecked {
            if (x + 3 < y + 5) r = 1;
        }
        if (b) r += y + 5;
    }
}
