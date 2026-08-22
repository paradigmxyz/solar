//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: callExternal() => true
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_v2.sol

contract AbiEncodeCallV2 {
    type UnsignedNumber is uint256;
    enum Enum {
        First,
        Second,
        Third
    }

    struct Struct {
        UnsignedNumber[] dynamicArray;
        uint256 justAnInt;
        string name;
        bytes someBytes;
        Enum theEnum;
    }

    function callMeMaybe(Struct calldata data, int256 intVal, string memory nameVal) external pure {
        assert(data.dynamicArray.length == 3);
        assert(UnsignedNumber.unwrap(data.dynamicArray[0]) == 0);
        assert(UnsignedNumber.unwrap(data.dynamicArray[1]) == 1);
        assert(UnsignedNumber.unwrap(data.dynamicArray[2]) == 2);
        assert(data.justAnInt == 6);
        assert(keccak256(bytes(data.name)) == keccak256("StructName"));
        assert(keccak256(data.someBytes) == keccak256(bytes("1234")));
        assert(data.theEnum == Enum.Second);
        assert(intVal == 5);
        assert(keccak256(bytes(nameVal)) == keccak256("TestName"));
    }

    function callExternal() public returns (bool) {
        Struct memory structToSend;
        structToSend.dynamicArray = new UnsignedNumber[](3);
        structToSend.dynamicArray[0] = UnsignedNumber.wrap(0);
        structToSend.dynamicArray[1] = UnsignedNumber.wrap(1);
        structToSend.dynamicArray[2] = UnsignedNumber.wrap(2);
        structToSend.justAnInt = 6;
        structToSend.name = "StructName";
        structToSend.someBytes = bytes("1234");
        structToSend.theEnum = Enum.Second;

        (bool success,) = address(this).call(
            abi.encodeCall(this.callMeMaybe, (structToSend, 5, "TestName"))
        );

        return success;
    }
}
