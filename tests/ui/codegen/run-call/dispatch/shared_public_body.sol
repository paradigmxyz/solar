//@ codegen-matrix: standard
//@ run-call: SharedPublicBody::shared 5 => 6, 7
//@ run-call: SharedPublicBody::invoke 5 => 6, 7
//@ run-call: SharedPublicBody::fib 8 => 21

contract SharedPublicBody {
    uint256 private bias;

    modifier around() {
        bias = 1;
        _;
        bias += 10;
    }

    function shared(uint256 value) public around returns (uint256 first, uint256 second) {
        first = value + bias;
        second = first + 1;
    }

    function invoke(uint256 value) external returns (uint256, uint256) {
        return shared(value);
    }

    function fib(uint256 value) public pure returns (uint256) {
        if (value < 2) return value;
        return fib(value - 1) + fib(value - 2);
    }
}
