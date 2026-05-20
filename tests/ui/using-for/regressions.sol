//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/enums/using_contract_enums_with_explicit_contract_name.sol
// ported-from: test/libsolidity/semanticTests/enums/using_inherited_enum_excplicitly.sol
// ported-from: test/libsolidity/semanticTests/using/imported_functions.sol
// ported-from: test/libsolidity/semanticTests/errors/using_structs.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/253_using_for_function_exists.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/254_using_for_function_on_int.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/491_using_this_in_constructor.sol
// ported-from: test/libsolidity/syntaxTests/using/global_local_clash.sol

import {S, f1 as f, gen, inc as aliasedInc} from "./auxiliary/regressions_imports.sol";
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

contract AttachedMemberClash {
    using {f} for S;

    function test() public pure returns (uint256) {
        return gen().f(); //~ ERROR: member `f` not unique
    }
}

struct StorageStringStruct {
    uint256 a;
    string b;
}

contract StorageStringAssignment {
    StorageStringStruct s;

    function f() public {
        s.a = 9;
        s.b = "abc";
    }
}

contract ConstructorThisWarning {
    constructor() {
        this.f(); //~ WARN: `this` used in constructor
    }

    function f() public pure {}
}

library PureWarningLib {
    function double(uint256 self) public returns (uint256) { //~ WARN: function state mutability can be restricted to `pure`
        return 2 * self;
    }
}

contract PureWarningMemberOnly {
    using PureWarningLib for uint256;

    function f(uint256 a) public { //~ WARN: function state mutability can be restricted to `pure`
        a.double;
    }
}

contract NoPureWarningForNonPureUsingCall {
    using PureWarningLib for uint256;

    function f(uint256 a) public returns (uint256) {
        return a.double();
    }
}
