//@ codegen-matrix: standard
//@ run-call: FractionalDenominations::g => 1500000000
//@ run-call: FractionalDenominations::e => 1500000000000000000
//@ run-call: FractionalDenominations::m => 90
//@ run-call: FractionalDenominations::h => 5400
//@ run-call: FractionalDenominations::d => 129600
//@ run-call: FractionalDenominations::w => 907200
//@ run-call: ScientificNotation::f => 20000000000
//@ run-call: ScientificNotation::g => 2
//@ run-call: ScientificNotation::h => 25
//@ run-call: ScientificNotation::i => -20000000000
//@ run-call: ScientificNotation::j => -2
//@ run-call: ScientificNotation::k => -25
// ported-from: test/libsolidity/semanticTests/literals/fractional_denominations.sol
// ported-from: test/libsolidity/semanticTests/literals/scientific_notation.sol

contract FractionalDenominations {
    uint256 public g = 1.5 gwei;
    uint256 public e = 1.5 ether;
    uint256 public m = 1.5 minutes;
    uint256 public h = 1.5 hours;
    uint256 public d = 1.5 days;
    uint256 public w = 1.5 weeks;
}

contract ScientificNotation {
    function f() public pure returns (uint256) {
        return 2e10 wei;
    }

    function g() public pure returns (uint256) {
        return 200e-2 wei;
    }

    function h() public pure returns (uint256) {
        return 2.5e1;
    }

    function i() public pure returns (int256) {
        return -2e10;
    }

    function j() public pure returns (int256) {
        return -200e-2;
    }

    function k() public pure returns (int256) {
        return -2.5e1;
    }
}
