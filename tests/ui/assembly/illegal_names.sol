// ported-from: test/libsolidity/syntaxTests/inlineAssembly/invalid/illegal_names.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/shadowsBuiltin/illegal_names_assembly_functions.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/shadowsBuiltin/illegal_names_assembly_identifier.sol

contract C {
    function f() public {
        // reserved function names
        assembly {
            function this() { //~ ERROR: identifier name `this` is reserved
            }
            function super() { //~ ERROR: identifier name `super` is reserved
            }
            function _() { //~ ERROR: identifier name `_` is reserved
            }
        }

        // reserved names as function argument
        assembly {
            function a(this) { //~ ERROR: identifier name `this` is reserved
            }
            function b(super) { //~ ERROR: identifier name `super` is reserved
            }
            function c(_) { //~ ERROR: identifier name `_` is reserved
            }
        }

        // reserved names as function return parameter
        assembly {
            function d() -> this { //~ ERROR: identifier name `this` is reserved
            }
            function g() -> super { //~ ERROR: identifier name `super` is reserved
            }
            function c() -> _ { //~ ERROR: identifier name `_` is reserved
            }
        }

        // reserved names as variable declaration
        assembly {
            let this := 1 //~ ERROR: identifier name `this` is reserved
            //~^ ERROR: this declaration shadows a declaration outside the inline assembly block
            let super := 1 //~ ERROR: identifier name `super` is reserved
            //~^ ERROR: this declaration shadows a declaration outside the inline assembly block
            let _ := 1 //~ ERROR: identifier name `_` is reserved
        }
    }
}
