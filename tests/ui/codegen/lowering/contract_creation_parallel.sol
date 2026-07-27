//@ revisions: sequential parallel2 parallel3 parallel4 parallel5 parallel6 parallel7 parallel8
//@[sequential] compile-flags: -j1
//@[parallel2]  compile-flags: -j2
//@[parallel3]  compile-flags: -j3
//@[parallel4]  compile-flags: -j4
//@[parallel5]  compile-flags: -j5
//@[parallel6]  compile-flags: -j6
//@[parallel7]  compile-flags: -j7
//@[parallel8]  compile-flags: -j8
//@ run-call: Root::create => 14

contract Leaf {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract Left {
    function create() external returns (uint256) {
        Leaf leaf = new Leaf();
        return leaf.value();
    }
}

contract Right {
    function create() external returns (uint256) {
        Leaf leaf = new Leaf();
        return leaf.value();
    }
}

contract Root {
    function create() external returns (uint256) {
        Left left = new Left();
        Right right = new Right();
        return left.create() + right.create();
    }
}

contract Independent {
    function value() external pure returns (uint256) {
        return 1;
    }
}
