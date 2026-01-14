interface MyInterface {
    enum MyEnum { E1, E2 }
}

contract Test {
    function testFunction(string memory) external returns (uint) {}

    event CustomEvent1(
        uint256,
        bool,
        bool[],
        address payable,
        MyInterface,
        MyInterface.MyEnum,
        function (string memory) external returns (uint)
    );


    function test() public {
        MyInterface instance = MyInterface(msg.sender);
        bool[] calldata arr;
        address payable addr;
        bytes4(CustomEvent1);
        bytes4(CustomEvent1());
        bytes4(CustomEvent1(1, true, arr, addr, instance, MyInterface.MyEnum.E1, this.testFunction));
        address(CustomEvent1);
    }
}
