contract Complex {
    struct A {
        B b;
    }

    struct B {
        uint256[] arr;
    }

    mapping(uint256 => A) public mapA;

    function pushValueA(uint256 idx, uint256 val) public {
        mapA[idx].b.arr.push(val);
    }

    mapping(uint256 => B) public mapB; //~ ERROR: getter must return at least one value

    function pushValueB(uint256 idx, uint256 val) public {
        mapB[idx].arr.push(val);
    }
}
