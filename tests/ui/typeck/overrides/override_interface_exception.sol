// Tests for interface override exception (when override is optional vs required)
// Based on solc tests: interfaceException/*.sol

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

// ==== Valid: abstract contract with override ====
contract GoodAbstractImpl is AbstractBase {
    function f() external override {}
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

// ==== Valid: diamond interfaces with proper override list ====
contract GoodDiamondImpl is IDiamond1, IDiamond2 {
    function f() external override(IDiamond1, IDiamond2) {}
    function g() external override(IDiamond1, IDiamond2) {}
    function h() external override(IDiamond1, IDiamond2) {}
}

// ==== Valid: interface inheriting interface - still optional for single chain ====
interface IParent {
    function x() external;
}
interface IChild is IParent {
    function y() external;
}
contract GoodInheritedImpl is IChild {
    function x() external {}  // optional override
    function y() external {}
}

// ==== Valid: diamond formed through interface inheritance - single base is OK ====
interface ILeft is IParent {}
interface IRight is IParent {}
contract GoodInheritedDiamond is ILeft, IRight {
    // Single shared base interface doesn't require explicit override
    function x() external {}
}

// ==== Valid: can also use explicit override ====
contract GoodInheritedDiamond2 is ILeft, IRight {
    function x() external override(IParent) {}
}
