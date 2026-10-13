// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_ancestor_contract_module.sol

import "./auxiliary/storage_layout_inheritance_m.sol" as M;
import "./auxiliary/storage_layout_inheritance_n.sol" as N;

contract C is M.A, N.A layout at 0xABCD {} //~ ERROR: cannot inherit from a contract with a custom storage layout
