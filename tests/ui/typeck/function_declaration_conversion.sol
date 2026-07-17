// ported-from: test/libsolidity/syntaxTests/functionTypes/declaration_type_conversion.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/ternary_with_attached_functions.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/from_ternary_expression.sol

library L {
    function f(uint256) internal pure {}
    function g(uint256) internal pure {}
    function publicF(uint256) public pure {}
    function publicG(uint256) public pure {}
}

contract D {
    function f() external {}
    function g() external {}
}

contract Base {
    function publicF(uint256) public pure {}
    function publicG(uint256) public pure {}
}

contract C is Base {
    using L for uint256;

    function internalF() internal pure {}
    function internalG() internal pure {}

    function f(bool condition, D d) public pure {
        (condition ? D.f : D.g);
        //~^ ERROR: invalid true type
        //~| ERROR: invalid false type

        uint256 value;
        (condition ? value.f : value.g)();
        //~^ ERROR: invalid true type
        //~| ERROR: invalid false type

        uint256 result = (condition ? addmod : addmod)(3, 4, 5);
        //~^ ERROR: invalid true type
        //~| ERROR: invalid false type

        function() internal pure mobile = condition ? internalF : internalG;
        mobile();

        function() external externalMobile = condition ? d.f : d.g;
        externalMobile;

        (condition ? address(0).call : address(0).call);
        //~^ ERROR: invalid true type
        //~| ERROR: invalid false type

        (condition ? new D : new D);
        //~^ ERROR: invalid true type
        //~| ERROR: invalid false type

        result;
    }

    function normalized(bool condition) public pure {
        function(uint256) internal pure mobile = condition ? Base.publicF : Base.publicG;
        mobile(1);

        (condition ? Base.publicF : Base.publicG).selector;
        //~^ ERROR: member `selector` not found

        (condition ? L.publicF : L.publicG).selector;
        //~^ ERROR: member `selector` not found
    }
}
