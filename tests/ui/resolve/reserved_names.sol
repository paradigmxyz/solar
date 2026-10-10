// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/shadowsBuiltin/illegal_names_function_parameters.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/shadowsBuiltin/this_super.sol
// ported-from: test/libsolidity/syntaxTests/parsing/placeholder_in_function_context.sol
// ported-from: test/libsolidity/syntaxTests/underscore/in_modifier.sol
// ported-from: test/libsolidity/syntaxTests/duplicateFunctions/illegal_names_exception.sol
// ported-from: test/libsolidity/syntaxTests/events/illegal_names_exception.sol

contract IllegalNamesFunctionParameters {
    function f(uint super) public { //~ ERROR: the name `super` is reserved
    }
    function g(uint this) public { //~ ERROR: the name `this` is reserved
    }
    function h(uint _) public { //~ ERROR: the name `_` is reserved
    }
    function i() public returns (uint super) { //~ ERROR: the name `super` is reserved
        return 1;
    }
    function j() public returns (uint this) { //~ ERROR: the name `this` is reserved
        return 1;
    }
    function k() public returns (uint _) { //~ ERROR: the name `_` is reserved
        return 1;
    }
}

contract ThisSuper {
    function f() pure public {
        uint super = 3; //~ ERROR: the name `super` is reserved
        uint this = 4; //~ ERROR: the name `this` is reserved
    }
}

contract PlaceholderInFunctionContext {
    function fun() public returns (uint r) {
        uint _ = 8; //~ ERROR: the name `_` is reserved
        return _ + 1;
    }
}

contract InModifier {
    modifier m() {
        _;
    }

    modifier n() {
        string memory _ = ""; //~ ERROR: the name `_` is reserved
        _;
        revert(_);
    }

    function f() m() public {
    }

    function g() n() public {
    }
}

// Exception for the rule about illegal names.
contract IllegalFunctionNamesException {
	function this() public {
	}
	function super() public {
	}
	function _() public {
	}
}

// Exception for the illegal name list. External interface events
contract IllegalEventNamesException {
	event this();
	event super();
	event _();
}
