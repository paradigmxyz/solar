//@check-pass
// A `public constant` in a library exposes both the constant and its
// auto-generated getter as members. Accessing it must resolve to the variable,
// not report an ambiguity.
library Errors {
    string public constant A = "1";
    uint256 public constant N = 7;
}

contract C {
    function s() external pure returns (string memory) {
        return Errors.A;
    }
    function n() external pure returns (uint256) {
        return Errors.N;
    }
}
