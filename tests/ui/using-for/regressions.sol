//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/enums/using_contract_enums_with_explicit_contract_name.sol
// ported-from: test/libsolidity/semanticTests/enums/using_inherited_enum_excplicitly.sol
// ported-from: test/libsolidity/semanticTests/using/imported_functions.sol

import {inc as aliasedInc} from "./auxiliary/regressions_imports.sol";
import "./auxiliary/regressions_imports.sol" as Imports;

using {Imports.inc, aliasedInc} for uint256;

contract EnumOwner {
    enum Choice {
        A,
        B,
        C
    }

    function answer() public pure returns (EnumOwner.Choice ret) {
        ret = EnumOwner.Choice.B;
    }
}

contract EnumBase {
    enum Choice {
        A,
        B,
        C
    }
}

contract EnumChild is EnumBase {
    function answer() public pure returns (EnumBase.Choice ret) {
        ret = EnumBase.Choice.B;
    }
}

contract ImportedFunctions {
    function f(uint256 x) public pure returns (uint256) {
        return x.inc() + x.aliasedInc();
    }
}
