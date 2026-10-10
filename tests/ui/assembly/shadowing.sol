// ported-from: test/libsolidity/syntaxTests/inlineAssembly/shadowing/argument.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/shadowing/contract.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/shadowing/function.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/shadowing/local_variable.sol

contract C {
    uint s;

    function argument(uint a) public pure {
        assembly {
            let a := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
        }
    }

    function contract_() public pure {
        assembly {
            let C := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
        }
    }

    function function_() public pure {
        assembly {
            let function_ := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
        }
    }

    function localVariable() public pure {
        uint a;
        assembly {
            let a := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
        }
    }

    function others() public pure returns (uint r) {
        assembly {
            let s := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
            let msg := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
            function g() -> x {
                let r := 1 //~ ERROR: this declaration shadows a declaration outside the inline assembly block
            }
        }
    }

    function external_() external {}

    function allowed(uint p) public pure returns (uint r) {
        assembly {
            let external_ := 1
            let later := 1
            function g(p) -> r {}
        }
        uint later;
    }
}
