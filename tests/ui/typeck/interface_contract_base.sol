// ported-from: test/libsolidity/syntaxTests/inheritance/interface/contract_base.sol

contract C {}
interface I is C {} //~ ERROR: interfaces can only inherit from other interfaces
