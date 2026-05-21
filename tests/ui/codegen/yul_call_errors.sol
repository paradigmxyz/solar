//@compile-flags: --emit=mir

contract YulCallErrors {
    uint256 storageValue;

    function unknownCall() public pure returns (uint256 result) {
        assembly {
            result := unknown_yul_call() //~ ERROR: undefined Yul function `unknown_yul_call`
        }
    }

    function unsupportedBuiltin() public pure returns (uint256 result) {
        assembly {
            result := addmod(1, 2, 3) //~ ERROR: unsupported Yul builtin `addmod`
        }
    }

    function unsupportedFunction() public pure returns (uint256 result) {
        assembly {
            result := id(1) //~ ERROR: unsupported Yul function `id`
            function id(x) -> y { //~ ERROR: unsupported Yul function definition
                y := x
            }
        }
    }

    function wrongArity() public pure {
        assembly {
            mstore(0x00) //~ ERROR: wrong number of arguments for Yul builtin `mstore`: expected 2, found 1
        }
    }

    function unsupportedFor() public pure {
        assembly {
            for { } 1 { } { } //~ ERROR: unsupported Yul for statement
        }
    }

    function undefinedVariable() public pure returns (uint256 result) {
        assembly {
            result := missing //~ ERROR: undefined Yul variable `missing`
        }
    }

    function unsupportedMultiVarDecl() public pure {
        assembly {
            let a, b := add(1, 2) //~ ERROR: unsupported Yul multiple variable declaration
        }
    }

    function unsupportedSlotAssign() public {
        assembly {
            storageValue.slot := 1 //~ ERROR: unsupported Yul storage slot assignment target
            storageValue.offset := 1 //~ ERROR: unsupported Yul storage offset assignment target
        }
    }
}
