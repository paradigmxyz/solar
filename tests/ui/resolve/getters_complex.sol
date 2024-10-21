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

    // TODO: Internal or recursive type is not allowed for public state variables.
    mapping(uint256 => B) public mapB;

    function pushValueB(uint256 idx, uint256 val) public {
        mapB[idx].arr.push(val);
    }
}
