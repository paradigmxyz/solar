// ported-from: test/libsolidity/syntaxTests/inlineAssembly/constant_bytes_ref.sol

contract C {
    bytes32 constant x = keccak256("abc");
    bytes32 constant y = x;

    uint constant three = 1 + 2;
    uint constant threeRef = three;
    uint constant threeRefRef = threeRef;
    uint constant folded = (2 ** 8) / 4 << 1;
    uint constant denominated = 1 ether;
    int constant negative = -1;
    bool constant boolean = true;
    bytes32 constant str = "abc";
    uint constant converted = uint(1);
    uint constant conditional = true ? 1 : 1;
    string constant text = "abc";
    bytes32 constant parenthesizedStr = ("abc");
    uint constant typedArithmetic = three + 1;
    uint constant bitNot = ~uint(0);

    function f() public pure returns (uint t) {
        assembly {
            t := y //~ ERROR: only direct number constants are supported in inline assembly
        }
    }

    function others() public pure returns (uint t) {
        assembly {
            t := three
            t := threeRef
            t := threeRefRef
            t := folded
            t := denominated
            t := negative
            t := boolean
            t := str
            t := converted //~ ERROR: only direct number constants are supported in inline assembly
            t := conditional //~ ERROR: only direct number constants are supported in inline assembly
            t := text //~ ERROR: only direct number constants are supported in inline assembly
            t := parenthesizedStr //~ ERROR: only direct number constants are supported in inline assembly
            t := typedArithmetic //~ ERROR: only direct number constants are supported in inline assembly
            t := bitNot //~ ERROR: only direct number constants are supported in inline assembly
        }
    }
}
