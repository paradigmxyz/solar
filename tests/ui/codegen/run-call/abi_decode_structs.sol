//@ run-call: dFlat(bytes) 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001 => 7, 0x0000000000000000000000000000000000000001, true
//@ run-call: nestedRoundtrip() => 7, "hi", 3
//@ run-call: dynamicArrayRoundtrip() => 2, 42, 43, 2
//@ run-call: dDynArr(bytes) 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000140000000000000000000000000000000000000000000000000000000000000002a000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000026869000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000005000000000000000000000000000000000000000000000000000000000000002b000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000003627965000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000006 => 2, 42, 43, 2

//@ run-call: fixedDynamicRoundtrip() => 1, 2
contract AbiDecodeStructsRunCall {
    struct Flat {
        uint256 a;
        address b;
        bool c;
    }

    struct Dyn {
        uint256 id;
        string name;
        uint256[] nums;
    }

    struct Nested {
        Flat flat;
        Dyn dyn;
        bytes tail;
    }

    function dFlat(bytes memory b) public pure returns (uint256, address, bool) {
        Flat memory f = abi.decode(b, (Flat));
        return (f.a, f.b, f.c);
    }

    function dNested(bytes memory b) public pure returns (uint256, string memory, uint256) {
        Nested memory n = abi.decode(b, (Nested));
        return (n.flat.a, n.dyn.name, n.tail.length);
    }

    function dDynArr(bytes memory b) public pure returns (uint256, uint256, uint256, uint256) {
        Dyn[] memory ds = abi.decode(b, (Dyn[]));
        return (ds.length, ds[0].id, ds[1].id, ds[0].nums.length);
    }

    function nestedRoundtrip() public pure returns (uint256, string memory, uint256) {
        Nested memory n;
        n.flat = Flat(7, address(1), true);
        n.dyn.id = 42;
        n.dyn.name = "hi";
        n.dyn.nums = new uint256[](2);
        n.dyn.nums[0] = 4;
        n.dyn.nums[1] = 5;
        n.tail = "xyz";
        return dNested(abi.encode(n));
    }

    function dynamicArrayRoundtrip() public pure returns (uint256, uint256, uint256, uint256) {
        Dyn[] memory ds = new Dyn[](2);
        Dyn memory first;
        first.id = 42;
        first.name = "hi";
        first.nums = new uint256[](2);
        first.nums[0] = 4;
        first.nums[1] = 5;
        Dyn memory second;
        second.id = 43;
        second.name = "bye";
        second.nums = new uint256[](1);
        second.nums[0] = 6;
        ds[0] = first;
        ds[1] = second;
        return dDynArr(abi.encode(ds));
    }

    function fixedDynamicRoundtrip() public pure returns (uint256, uint256) {
        bytes[2] memory values;
        values[0] = hex"01";
        values[1] = hex"0203";
        bytes[2] memory decoded = abi.decode(abi.encode(values), (bytes[2]));
        return (decoded[0].length, decoded[1].length);
    }
}
