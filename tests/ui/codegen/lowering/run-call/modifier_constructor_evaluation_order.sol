//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: D::query() => [4, 2, 6, 1, 3, 5, 7]
//@[gas] run-call: D::query() => [4, 2, 6, 1, 3, 5, 7]
//@[size] run-call: D::query() => [4, 2, 6, 1, 3, 5, 7]
// ported-from: test/libsolidity/semanticTests/modifiers/evaluation_order.sol

contract EvaluationOrderA {
    constructor(uint256) {}
}

contract EvaluationOrderB {
    constructor(uint256) {}
}

contract EvaluationOrderC {
    constructor(uint256) {}
}

contract D is EvaluationOrderA, EvaluationOrderB, EvaluationOrderC {
    uint256[] private values;

    constructor()
        m2(f(1))
        EvaluationOrderB(f(2))
        m1(f(3))
        EvaluationOrderC(f(4))
        m3(f(5))
        EvaluationOrderA(f(6))
    {
        f(7);
    }

    function query() external view returns (uint256[] memory) {
        return values;
    }

    modifier m1(uint256) {
        _;
    }

    modifier m2(uint256) {
        _;
    }

    modifier m3(uint256) {
        _;
    }

    function f(uint256 value) internal returns (uint256) {
        values.push(value);
        return 0;
    }
}
