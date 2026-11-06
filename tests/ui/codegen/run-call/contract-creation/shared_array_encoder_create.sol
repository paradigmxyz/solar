//@ codegen-matrix: standard
//@ run-call: second => 0x0000000000000000000000000000000000000004
//@ run-call: both => 2, 4
//@ run-call: lengths => 2, 3

// Two creation sites encode an `address[]` constructor argument, so the
// encoder shares one per-type helper between them. The encoded arguments
// sit above the free-memory pointer until the whole payload is measured, so
// the helper call must not allocate or spill into that region.

contract Child {
    address[] public xs;

    constructor(address[] memory a) {
        xs = a;
    }

    function get(uint256 i) external view returns (address) {
        return xs[i];
    }

    function len() external view returns (uint256) {
        return xs.length;
    }
}

contract C {
    function mk(address a, address b) internal returns (Child) {
        address[] memory arr = new address[](2);
        arr[0] = a;
        arr[1] = b;
        return new Child(arr);
    }

    function mk3(address a, address b, address c) internal returns (Child) {
        address[] memory arr = new address[](3);
        arr[0] = a;
        arr[1] = b;
        arr[2] = c;
        return new Child(arr);
    }

    function second() external returns (address) {
        Child c1 = mk(address(1), address(2));
        Child c2 = mk(address(3), address(4));
        c1.get(0);
        return c2.get(1);
    }

    function both() external returns (uint256, uint256) {
        Child c1 = mk(address(1), address(2));
        Child c2 = mk3(address(3), address(4), address(5));
        return (uint160(c1.get(1)), uint160(c2.get(1)));
    }

    function lengths() external returns (uint256, uint256) {
        Child c1 = mk(address(1), address(2));
        Child c2 = mk3(address(3), address(4), address(5));
        return (c1.len(), c2.len());
    }
}
