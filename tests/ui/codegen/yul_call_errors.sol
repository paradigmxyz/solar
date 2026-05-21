//@compile-flags: --emit=mir

contract YulCallErrors {
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
            function id(x) -> y {
                y := x
            }
            result := id(1) //~ ERROR: unsupported Yul function `id`
        }
    }
}
