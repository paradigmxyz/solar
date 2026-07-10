// ported-from: test/libsolidity/syntaxTests/inheritance/override/interfaceException/abstract_needed.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/interfaceException/diamond_needed.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/interfaceException/regular_optional.sol

// ==== Valid: single interface - override is optional ====
interface ISingle {
    function f() external;
    function g() external;
    function h() external;
}
contract GoodSingleImpl is ISingle {
    // All three forms are valid when implementing a single interface
    function f() external {}               // no override - OK
    function g() external override {}      // with override - OK
    function h() external override(ISingle) {}  // explicit interface - OK
}

// ==== Invalid: abstract contract requires override ====
abstract contract AbstractBase {
    function f() external virtual;
}
contract BadAbstractImpl is AbstractBase {
    function f() external {}
    //~^ ERROR: overriding function is missing `override` specifier
}

// ==== Invalid: diamond interfaces require explicit override list ====
interface IDiamond1 {
    function f() external;
    function g() external;
    function h() external;
}
interface IDiamond2 {
    function f() external;
    function g() external;
    function h() external;
}
contract BadDiamondImpl is IDiamond1, IDiamond2 {
    function f() external {}
    //~^ ERROR: Function needs to specify overridden contracts
    function g() external override {}
    //~^ ERROR: Function needs to specify overridden contracts
    function h() external override(IDiamond1) {}
    //~^ ERROR: Function needs to specify overridden contracts
}
