//@compile-flags: -Ztypeck

library CallLib {
    struct Helper {
        uint256 value;
    }

    struct Text {
        string value;
    }

    function id(uint256 value) internal pure returns (uint256) {
        return value;
    }
}

contract StaticBase {
    function internalId(uint256 value) internal pure returns (uint256) {
        return value;
    }
}

contract CallChecking is StaticBase {
    event E(uint a, bytes32 b);
    event EmptyEvent();
    error MyError(uint code, bytes32 message);
    error EmptyError();

    struct MyStruct {
        uint x;
        bytes32 y;
    }

    function target(uint x, bytes32 y) public pure {}
    function noArgs() public pure returns (uint256) {
        return 42;
    }
    function multiReturn() public pure returns (uint, bytes32) {
        return (1, "hi");
    }

    function overloaded(uint256) internal pure returns (bool) {
        return true;
    }

    function overloaded(uint256, uint256) internal pure returns (bool) {
        return true;
    }

    function privateView(uint256) private view {}

    function testBuiltinsAndLibraryMembers() public pure {
        uint256[] memory values = new uint256[](2);
        string memory s = string.concat("a", "b", "c");
        bytes memory b = bytes.concat(hex"12", hex"34");
        uint256 value = CallLib.id(values.length);
        CallLib.Helper memory helper = CallLib.Helper({value: value});
        CallLib.Text memory text = CallLib.Text({value: "text"});
        require(value == StaticBase.internalId(0), "value");
        revert("reason");
        s;
        b;
        helper;
        text;
    }

    function testConversions() public pure returns (address, bool) {
        return (address(0), bytes4(0x01ffc9a7) == 0x01ffc9a7);
    }

    function testOverloadAndFunctionPointer() public view {
        function(uint256) internal view fnIn = privateView;
        overloaded(1);
        overloaded(1, 2);
        fnIn;
    }

    // === Correct positional arguments ===
    function testPositional() public pure {
        target(1, "hi");
        noArgs();
        multiReturn();
    }

    // === Zero-arg function/event/error calls ===
    function testZeroArgs() public pure {
        noArgs();
    }
    function testEmptyEvent() public {
        emit EmptyEvent();
    }
    function testEmptyError() public pure {
        revert EmptyError();
    }

    // === Wrong argument count ===
    function testEmptyEventWrongArgs() public {
        emit EmptyEvent(1); //~ ERROR: wrong argument count
    }
    function testEmptyErrorWrongArgs() public pure {
        revert EmptyError(1); //~ ERROR: wrong argument count
    }
    function testWrongCount() public pure {
        target(1); //~ ERROR: wrong argument count
    }
    function testWrongCountTooMany() public pure {
        target(1, "hi", 3); //~ ERROR: wrong argument count
    }

    // === Wrong type ===
    function testWrongType() public pure {
        target("hi", 1);
        //~^ ERROR: mismatched types
        //~| ERROR: mismatched types
    }

    // === Named arguments - correct ===
    function testNamedCorrect() public pure {
        target({x: 1, y: "hi"});
        target({y: "hi", x: 1});
    }

    // === Named arguments - duplicate ===
    function testNamedDuplicate() public pure {
        target({x: 1, x: 2, y: "hi"});
        //~^ ERROR: wrong argument count
        //~| ERROR: duplicate named argument
    }

    // === Named arguments - invalid name ===
    function testNamedInvalidName() public pure {
        target({x: 1, z: "hi"});
        //~^ ERROR: named argument `z` does not match function declaration
    }

    // === Named arguments - wrong count ===
    function testNamedWrongCount() public pure {
        target({x: 1}); //~ ERROR: wrong argument count
    }

    // === Event emit - correct ===
    function testEventCorrect() public {
        emit E(1, "hi");
        emit E({a: 1, b: "hi"});
        emit E({b: "hi", a: 1});
    }

    // === Event emit - wrong count ===
    function testEventWrongCount() public {
        emit E({a: 1}); //~ ERROR: wrong argument count
    }

    // === Event emit - named arg errors ===
    function testEventNamedErrors() public {
        emit E({a: 1, a: 2, b: "hi"});
        //~^ ERROR: wrong argument count
        //~| ERROR: duplicate named argument
    }

    // === Error/revert - correct ===
    function testRevertCorrect() public pure {
        revert MyError(404, "not found");
        revert MyError({code: 404, message: "not found"});
    }

    // === Error/revert - wrong count ===
    function testRevertWrongCount() public pure {
        revert MyError(404); //~ ERROR: wrong argument count
    }

    // === Error/revert - named argument errors ===
    function testRevertNamedErrors() public pure {
        revert MyError({code: 1, code: 2, message: "hi"});
        //~^ ERROR: wrong argument count
        //~| ERROR: duplicate named argument
        revert MyError({code: 1, msg: "hi"});
        //~^ ERROR: named argument `msg` does not match function declaration
    }

    // === Not callable ===
    function testNotCallable() public {
        ((1(3)), 2);
        //~^ ERROR: expected function
    }

    // === Struct constructor ===
    function testStructConstructor() public pure {
        MyStruct(1, "hi");
        MyStruct({x: 1, y: "hi"});
        MyStruct({y: "hi", x: 1});
    }
    function testStructConstructorWrongCount() public pure {
        MyStruct(1); //~ ERROR: wrong argument count
    }
    function testStructConstructorWrongType() public pure {
        MyStruct("hi", 1);
        //~^ ERROR: mismatched types
        //~| ERROR: mismatched types
    }
}
