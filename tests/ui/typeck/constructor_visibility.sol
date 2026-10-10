// ported-from: test/libsolidity/syntaxTests/constructor/constructible_internal_constructor.sol
// ported-from: test/libsolidity/syntaxTests/constructor/inconstructible_internal_constructor.sol
// ported-from: test/libsolidity/syntaxTests/constructor/inconstructible_internal_constructor_inverted.sol
// ported-from: test/libsolidity/syntaxTests/constructor/internal_constructor_non_abstract.sol
// ported-from: test/libsolidity/syntaxTests/constructor/public_constructor_abstract.sol
// ported-from: test/libsolidity/syntaxTests/constructor/public_constructor_non_abstract.sol

abstract contract InternalAbstract {
	constructor() internal {} //~ WARN: visibility for constructor is ignored
}
contract DerivedFromInternalAbstract is InternalAbstract {
	constructor() { }
}

contract Internal {
	constructor() internal {} //~ ERROR: non-abstract contracts cannot have internal constructors
}
contract CreatesInternal {
	function f() public { Internal c = new Internal(); c; }
}

// Previously, the type information for A was not yet available at the point of
// "new A".
contract B {
	A a;
	constructor() {
		a = new A(address(this));
	}
}
contract A {
	constructor(address) internal {} //~ ERROR: non-abstract contracts cannot have internal constructors
}

contract InternalOnly {
	constructor() internal {} //~ ERROR: non-abstract contracts cannot have internal constructors
}

abstract contract PublicAbstract {
	constructor() public {} //~ ERROR: abstract contracts cannot have public constructors
}

contract Public {
	constructor() public {} //~ WARN: visibility for constructor is ignored
}

// The parser rejects these, and no visibility is assumed in their place.
contract External {
	constructor() external {} //~ ERROR: `external` not allowed here
}
contract Private {
	constructor() private {} //~ ERROR: `private` not allowed here
}
