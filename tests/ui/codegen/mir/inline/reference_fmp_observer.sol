//@ codegen-matrix: standard
//@ run-call: check 42, 0 => true
contract C {
    function wrap(uint x) private pure returns (bytes memory) { return abi.encode(x); }
    function observe(uint n) private pure returns (uint r) {
        assembly { r := mload(64) }
        for (uint i; i < n; ++i) r ^= i;
    }
    function check(uint x, uint n) external pure returns (bool) {
        uint before = observe(n);
        bytes memory b = wrap(x);
        uint value;
        assembly { value := mload(add(b, 32)) }
        uint after_ = observe(n);
        return after_ == before + 64 && value == x;
    }
}
