// ported-from: test/libsolidity/syntaxTests/using/global_for_type_from_other_file.sol

import {ImportedS, ImportedU} from "./auxiliary/imported_types.sol";

function idS(ImportedS memory s) pure returns (ImportedS memory) {
    return s;
}

function idU(ImportedU u) pure returns (ImportedU) {
    return u;
}

using {idS} for ImportedS global; //~ ERROR: can only use `global` with types defined in the same source unit at file level
using {idU} for ImportedU global; //~ ERROR: can only use `global` with types defined in the same source unit at file level
