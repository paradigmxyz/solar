// Tests for calldata/memory override variations
// Based on solc tests: calldata_memory_interface.sol, calldata_memory_struct.sol

// ==== Valid: interface calldata can be implemented with memory (external -> public) ====
interface ICalldataToMemory {
    function f(uint[] calldata) external pure;
    function g(uint[] calldata) external view;
    function h(uint[] calldata) external;
    function i(uint[] calldata) external payable;
}
contract Good1 is ICalldataToMemory {
    uint dummy;
    function f(uint[] memory) public pure {}
    function g(uint[] memory) public view { dummy; }
    function h(uint[] memory) public { dummy = 42; }
    function i(uint[] memory) public payable {}
}

// ==== Valid: external calldata base can be overridden with public memory ====
contract CalldataBase {
    uint dummy;
    struct S { int a; }
    function f(S calldata) external virtual pure {}
    function g(S calldata) external virtual view { dummy; }
    function h(S calldata) external virtual { dummy = 42; }
    function i(S calldata) external virtual payable {}
}
contract Good2 is CalldataBase {
    function f(S memory) public override pure {}
    function g(S memory) public override view { dummy; }
    function h(S memory) public override { dummy = 42; }
    function i(S memory) public override payable {}
}

// ==== Valid: same visibility, calldata to memory allowed ====
contract ExternalCalldataBase {
    function bar(uint[] calldata x) external virtual returns (uint[] calldata) { return x; }
}
contract Good3 is ExternalCalldataBase {
    function bar(uint[] memory x) public override returns (uint[] memory) { return x; }
}

// ==== Invalid: public base with memory cannot be overridden with calldata ====
contract MemoryBase {
    function foo(uint[] memory x) public virtual returns (uint[] memory) { return x; }
}
contract Bad1 is MemoryBase {
    function foo(uint[] calldata x) public override returns (uint[] memory) { return x; }
    //~^ ERROR: parameter data locations differ when overriding non-external function
}

// ==== Invalid: return location mismatch ====
contract Bad2 is MemoryBase {
    function foo(uint[] memory x) public override returns (uint[] calldata) {}
    //~^ ERROR: return variable data locations differ when overriding non-external function
}

// ==== Valid: bytes and string also follow the same rules ====
interface IBytesCalldata {
    function f(bytes calldata) external pure;
    function g(string calldata) external pure;
}
contract Good4 is IBytesCalldata {
    function f(bytes memory) public pure {}
    function g(string memory) public pure {}
}

// ==== Invalid: bytes memory base cannot accept calldata override ====
contract BytesMemoryBase {
    function f(bytes memory) public virtual {}
}
contract Bad3 is BytesMemoryBase {
    function f(bytes calldata) public override {}
    //~^ ERROR: parameter data locations differ when overriding non-external function
}
