type fixed8 is uint256;
type fixed8x1foo is uint256;
type fixed128x18 is uint256; //~ ERROR: expected identifier
type Bad is fixed; //~ ERROR: fixed-point types are not yet implemented
type Recursive is Recursive; //~ ERROR: the underlying type of UDVTs must be an elementary value type

contract C {
    fixed value; //~ ERROR: fixed-point types are not yet implemented
    ufixed128x18[] array; //~ ERROR: fixed-point types are not yet implemented
    mapping(fixed => uint256) map; //~ ERROR: fixed-point types are not yet implemented
    fixed immutable immutableValue; //~ ERROR: fixed-point types are not yet implemented
    fixed8 customValue;
    fixed8x1foo otherCustomValue;

    function f() internal pure {
        fixed128x18(1); //~ ERROR: fixed-point types are not yet implemented
    }

    function yulIdentifiers() internal pure {
        assembly {
            let fixed := 1
            let ufixed := 2
            let fixed128x18 := add(fixed, ufixed)
        }
    }
}
