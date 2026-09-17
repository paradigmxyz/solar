//@ codegen-matrix: standard
//@ run-call: branch true, 17, 29 => 17
//@ run-call: branch false, 17, 29 => 29
//@ run-call: accumulate 0 => 0
//@ run-call: accumulate 10 => 55
//@ run-call: fresh 4 => 10
//@ run-call: coupled true, 71 => 71
//@ run-call: coupled false, 71 => 0
contract AggregateControlFlow {
    struct Pair { uint256 x; uint256 y; }
    function branch(bool choice, uint256 a, uint256 b) external pure returns (uint256) {
        Pair memory p;
        if (choice) p.x = a; else p.x = b;
        return p.x;
    }
    function accumulate(uint256 n) external pure returns (uint256) {
        Pair memory p;
        for (uint256 i; i < n; ++i) p.x += i + 1;
        return p.x;
    }
    function fresh(uint256 n) external pure returns (uint256 result) {
        for (uint256 i; i < n; ++i) {
            Pair memory p;
            if (i % 2 == 0) p.x = 5;
            result += p.x;
        }
    }
    function coupled(bool choice, uint256 n) external pure returns (uint256) {
        Pair memory p;
        if (choice) { p.x = n; p.y = p.x; }
        return p.y;
    }
}
