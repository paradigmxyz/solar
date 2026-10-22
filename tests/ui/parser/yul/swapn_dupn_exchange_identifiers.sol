//@ revisions: osaka amsterdam
//@[osaka] compile-flags: --evm-version osaka
//@[amsterdam] compile-flags: --evm-version amsterdam

// ported-from: test/libyul/yulSyntaxTests/swapn_dupn_exchange_identifiers.yul
contract C {
    function f() external pure {
        assembly {
            function swapn(dupn) -> exchange {}
            //~^ WARN: `swapn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `dupn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `exchange` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        }
    }

    function g() external pure {
        assembly {
            let swapn
            //~^ WARN: `swapn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            let dupn
            //~^ WARN: `dupn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            let exchange
            //~^ WARN: `exchange` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        }
    }
}
