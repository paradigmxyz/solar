type fixed8 is uint256;
type fixed8x1foo is uint256;
type fixed128x18 is uint256; //~ ERROR: expected identifier

contract C {
    fixed value; //~ ERROR: fixed-point types are not yet implemented
    ufixed128x18 sizedValue; //~ ERROR: fixed-point types are not yet implemented
    fixed8 customValue;
    fixed8x1foo otherCustomValue;

    function cast() internal pure {
        fixed128x18(1); //~ ERROR: fixed-point types are not yet implemented
        type(fixed); //~ ERROR: fixed-point types are not yet implemented
        type(fixed[]); //~ ERROR: fixed-point types are not yet implemented
        new fixed[](1); //~ ERROR: fixed-point types are not yet implemented
    }

    function locals() internal pure {
        fixed128x18 scalar; //~ ERROR: fixed-point types are not yet implemented
        ufixed128x18[] memory array; //~ ERROR: fixed-point types are not yet implemented
        scalar;
        array;
    }

    function yulIdentifiers() internal pure {
        assembly {
            let fixed := 1
            let ufixed := 2
            let fixed128x18 := add(fixed, ufixed)
        }
    }
}
