// ported-from: test/libsolidity/syntaxTests/inlineAssembly/swapn_dupn_exchange_reserved_warnings.sol

contract C {
    function f() public pure {
        assembly {
            function swapn(dupn) -> exchange {}
            //~^ WARN: `swapn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `dupn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `exchange` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            let dupn
            //~^ WARN: `dupn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        }
    }
    function g() public pure {
        assembly {
            function dupn(exchange) -> swapn {}
            //~^ WARN: `dupn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `exchange` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `swapn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            let exchange
            //~^ WARN: `exchange` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        }
    }
    function h() public pure {
        assembly {
            function exchange(swapn) -> dupn {}
            //~^ WARN: `exchange` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `swapn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~| WARN: `dupn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            let swapn
            //~^ WARN: `swapn` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        }
    }
}
