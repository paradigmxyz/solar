//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: f() => 234

contract CreationEvaluationOrderChild {
    constructor(uint256) payable {}
}

contract CreationEvaluationOrder {
    uint256 marker;

    function valueOpt() internal returns (uint256) {
        marker = marker * 10 + 2;
        return 0;
    }

    function saltOpt() internal returns (bytes32) {
        marker = marker * 10 + 3;
        return bytes32(0);
    }

    function arg() internal returns (uint256) {
        marker = marker * 10 + 4;
        return 1;
    }

    function f() external returns (uint256) {
        new CreationEvaluationOrderChild{value: valueOpt(), salt: saltOpt()}(arg());
        return marker;
    }
}
