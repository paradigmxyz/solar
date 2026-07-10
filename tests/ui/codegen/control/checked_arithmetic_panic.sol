//@compile-flags: -Zcodegen --emit=mir

contract CheckedArithmeticPanic {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function sub(uint256 a, uint256 b) public pure returns (uint256) {
        return a - b;
    }

    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return a * b;
    }

    function div_zero(uint256 a, uint256 b) public pure returns (uint256) {
        return a / b;
    }

    function pow(uint256 a, uint256 b) public pure returns (uint256) {
        return a ** b;
    }

    function signed_add(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    function signed_neg(int256 a) public pure returns (int256) {
        return -a;
    }

    function narrow_add(uint8 a, uint8 b) public pure returns (uint8) {
        return a + b;
    }

    function unchecked_add(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return a + b;
        }
    }

    function unchecked_neg(int256 a) public pure returns (int256) {
        unchecked {
            return -a;
        }
    }

    function unchecked_pow(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return a ** b;
        }
    }

    function unchecked_call(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return checked_inner(a, b);
        }
    }

    function checked_inner(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}
