// ported-from: test/libsolidity/syntaxTests/inheritance/override/calldata_memory_interface.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/calldata_memory_struct.sol

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
