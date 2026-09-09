//@ codegen-matrix: standard
//@ run-call: vested(uint256,uint64) 100, 9; constructor=[10, 20] => 0
//@ run-call: vested(uint256,uint64) 100, 10; constructor=[10, 20] => 0
//@ run-call: vested(uint256,uint64) 100, 20; constructor=[10, 20] => 50
//@ run-call: vested(uint256,uint64) 100, 30; constructor=[10, 20] => 100
//@ run-call: vested(uint256,uint64) 100, 10; constructor=[10, 0] => 100
//@ run-call: initial(); constructor=[18446744073709551615, 18446744073709551615] => 36893488147419103230
//@ run-call: end(); constructor=[18446744073709551615, 18446744073709551615] => 36893488147419103230

contract ImmutableLeaves {
    uint64 private immutable start;
    uint64 private immutable duration;
    uint256 public immutable initial;

    constructor(uint64 s, uint64 d) {
        start = s;
        duration = d;
        initial = uint256(s) + uint256(d);
    }

    function end() public view returns (uint256) {
        return uint256(start) + uint256(duration);
    }

    function vested(uint256 amount, uint64 timestamp) public view returns (uint256) {
        if (timestamp < start) return 0;
        if (timestamp >= end()) return amount;
        return (amount * (timestamp - start)) / duration;
    }
}
