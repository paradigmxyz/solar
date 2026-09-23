//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: escape 0x73617920226869225c => 0x736179205c2268695c225c5c
//@ run-call: escape 0x6c696e650a627265616b09 => 0x6c696e655c6e627265616b5c74
//@ run-call: escape 0x011f => 0x5c75303030315c7530303166
//@ run-call: escape 0x => 0x

// Escaping for JSON through the compiler-owned `Strings.escapeJSON`, which
// streams the output at the free-memory pointer and reserves its exact size.
// No pointer arithmetic, no guess at the output's size.
// CHECK-LABEL: fn @escape
// CHECK: icall @core_string_escape_json,
import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function escape(bytes memory s) public pure returns (bytes memory) {
        return bytes(Strings.escapeJSON(string(s)));
    }
}
