//@ compile-flags: -Ztypeck

contract Base1 {
    constructor(uint x) {}
}

contract Base2 {
    constructor(int y) {}
}

// OK: abstract contracts can skip base constructor args
abstract contract AbstractDerived is Base1 {}

// Not OK: non-abstract contract must provide args
contract DerivedMissing is Base1 { } //~ ERROR: no arguments passed to the base constructor

// OK: args provided via inheritance specifier
contract DerivedWithArgs is Base1(42) { }

// OK: args provided via constructor modifier
contract DerivedWithCtorArgs is Base1 {
    constructor() Base1(100) {}
}

// === Multiple Inheritance ===

// ERROR: Missing args for Base2
contract Bad2 is Base1(1), Base2 {} //~ ERROR: no arguments passed to the base constructor

// OK: All args provided for multiple bases
contract Good2 is Base1(1), Base2(2) {}

// OK: Multiple bases with constructor modifiers
contract Good3 is Base1, Base2 {
    constructor() Base1(1) Base2(2) {}
}

// ERROR: Missing one base in multiple inheritance
contract Bad3 is Base1, Base2 { //~ ERROR: no arguments passed to the base constructor
    constructor() Base1(1) {}
}

// OK: Abstract can skip all args even with multiple bases
abstract contract AbstractMulti is Base1, Base2 {}

// === Deep Inheritance Chain ===

contract Grandparent {
    constructor(uint g) {}
}

// Parent satisfies Grandparent's constructor
contract Parent is Grandparent(10) {
    constructor(uint p) {}
}

// ERROR: Missing Parent's args (and Grandparent's due to current implementation - 2 errors)
contract BadChild is Parent {} //~ ERROR: no arguments passed to the base constructor
//~^ ERROR: no arguments passed to the base constructor

// OK: Child provides Parent's args (Grandparent satisfied by Parent)
// TODO: Currently errors incorrectly - Grandparent args should be satisfied by Parent
contract GoodChild is Parent(20) {} //~ ERROR: no arguments passed to the base constructor

// === Diamond Inheritance ===
// Note: solc actually reports "Base constructor arguments given twice" for this pattern
// Our implementation doesn't detect duplicate args yet

contract DiamondBase {
    constructor(uint d) {}
}

contract DiamondLeft is DiamondBase(1) {}
contract DiamondRight is DiamondBase(2) {}

// TODO: solc errors with "Base constructor arguments given twice"
// Our implementation incorrectly says no args passed
contract DiamondChild is DiamondLeft, DiamondRight {} //~ ERROR: no arguments passed to the base constructor

// === Interfaces (no constructor args needed) ===

interface IExample {
    function foo() external;
}

// OK: Interfaces don't have constructors
contract ImplementsInterface is IExample {
    function foo() external override {}
}

// OK: Multiple interfaces
interface IExample2 {}
contract ImplementsMultipleInterfaces is IExample, IExample2 {
    function foo() external override {}
}

// === Base with no constructor (implicit default) ===

contract NoConstructor {}

// OK: Base has no constructor
contract DerivedFromNoConstructor is NoConstructor {}

// === Mixed: Interface + Base with constructor ===

// ERROR: Missing Base1's args (interface doesn't help)
contract MixedBad is IExample, Base1 { //~ ERROR: no arguments passed to the base constructor
    function foo() external override {}
}

// OK: Provides Base1's args
contract MixedGood is IExample, Base1(42) {
    function foo() external override {}
}

// === Abstract base with constructor ===

abstract contract AbstractWithCtor {
    constructor(uint x) {}
}

// ERROR: Must provide args even though base is abstract
contract ConcreteFromAbstract is AbstractWithCtor {} //~ ERROR: no arguments passed to the base constructor

// OK: Provides args for abstract base
contract ConcreteFromAbstractOk is AbstractWithCtor(100) {}

// OK: Abstract child can skip args
abstract contract AbstractFromAbstract is AbstractWithCtor {}

// === Chain of abstracts ===

abstract contract AbstractA {
    constructor(uint a) {}
}

abstract contract AbstractB is AbstractA {
    constructor(uint b) {}
}

// ERROR: Must provide both args (current impl errors twice - see TODOs above)
contract ConcreteFromAbstractChain is AbstractB {} //~ ERROR: no arguments passed to the base constructor
//~^ ERROR: no arguments passed to the base constructor

// OK: Provides AbstractB's args (but current impl still errors for AbstractA - see TODOs)
contract ConcreteFromAbstractChainOk is AbstractB(1) {} //~ ERROR: no arguments passed to the base constructor
