// Tests for override checker (error codes: 7792, 4334, 9456, 4327, 6480, 3656, 9098, 6959, 4822)

contract Base {
    function mustOverride() public virtual returns (uint) { return 1; }
    function notVirtual() public returns (uint) { return 2; }
    //~^ ERROR: trying to override non-virtual function
    function viewFn() public view virtual returns (uint) { return 3; }
    function pureFn() public pure virtual returns (uint) { return 4; }
    function externalFn() external virtual returns (uint) { return 5; }
    function payableFn() public payable virtual returns (uint) { return 6; }
}

contract Base2 {
    function mustOverride() public virtual returns (uint) { return 10; }
}

interface IBase {
    function interfaceFn() external returns (uint);
}

// ERROR 9456: missing override specifier
contract Bad1 is Base {
    function mustOverride() public returns (uint) { return 100; }
    //~^ ERROR: overriding function is missing `override` specifier
}

// ERROR 4334: base not virtual (error is on Base.notVirtual line 5)
contract Bad2 is Base {
    function notVirtual() public override returns (uint) { return 200; }
}

// ERROR 4327: must specify all bases in multi-inheritance
contract Bad3 is Base, Base2 {
    function mustOverride() public override returns (uint) { return 300; }
    //~^ ERROR: Function needs to specify overridden contracts
}

// ERROR 7792: override without base function
contract Bad4 {
    function noBase() public override returns (uint) { return 400; }
    //~^ ERROR: Function has override specified but does not override anything
}

// ERROR 6480: diamond inheritance - must override conflicting function
contract Bad5 is Base, Base2 {}
//~^ ERROR: derived contract must override function `mustOverride`

// ERROR 3656: non-abstract with unimplemented function
contract Bad6 is IBase {}
//~^ ERROR: contract `Bad6` should be marked as abstract

// ERROR 9098: visibility compatibility
contract Bad7 is Base {
    function externalFn() internal override returns (uint) { return 500; }
    //~^ ERROR: overriding function visibility differs
}

// ERROR 6959: mutability compatibility (less strict)
contract Bad8 is Base {
    function viewFn() public override returns (uint) { return 600; }
    //~^ ERROR: overriding function changes state mutability from `view` to `nonpayable`
}

// ERROR 6959: payable cannot be overridden by non-payable
contract Bad9 is Base {
    function payableFn() public override returns (uint) { return 700; }
    //~^ ERROR: overriding function changes state mutability from `payable` to `nonpayable`
}

// ERROR 4822: return type mismatch
contract Bad10 is Base {
    function mustOverride() public override returns (int) { return 800; }
    //~^ ERROR: overriding function return types differ
}

// OK: proper single inheritance override
contract Good1 is Base {
    function mustOverride() public override returns (uint) { return 1000; }
}

// OK: multi-inheritance with proper specifier
contract Good2 is Base, Base2 {
    function mustOverride() public override(Base, Base2) returns (uint) { return 2000; }
}

// OK: abstract can have unimplemented
abstract contract Good3 is IBase {}

// OK: interface implementation with override
contract Good4 is IBase {
    function interfaceFn() external override returns (uint) { return 3000; }
}

// OK: stricter mutability (view overriding nonpayable)
contract Good5 is Base {
    function mustOverride() public view override returns (uint) { return 4000; }
}

// OK: pure overriding view
contract Good6 is Base {
    function viewFn() public pure override returns (uint) { return 5000; }
}

// OK: external can be overridden with public
contract Good7 is Base {
    function externalFn() public override returns (uint) { return 6000; }
}

// Tests for data location compatibility (error codes: 7723, 1443)
contract BasePublic {
    function foo(uint[] memory x) public virtual returns (uint[] memory) { return x; }
}

// ERROR 7723: parameter data location mismatch
contract Bad11 is BasePublic {
    function foo(uint[] calldata x) public override returns (uint[] memory) { return x; }
    //~^ ERROR: data locations of parameters have to be the same
}

// ERROR 1443: return data location mismatch
contract Bad12 is BasePublic {
    function foo(uint[] memory x) public override returns (uint[] calldata) {}
    //~^ ERROR: data locations of return variables have to be the same
}

// OK: external base allows calldata->memory conversion
contract BaseExternal {
    function bar(uint[] calldata x) external virtual returns (uint[] calldata) { return x; }
}

contract Good8 is BaseExternal {
    function bar(uint[] memory x) public override returns (uint[] memory) { return x; }
}
