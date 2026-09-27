//@ revisions: osaka amsterdam parse
//@[osaka] compile-flags: --evm-version osaka
//@[amsterdam] compile-flags: --evm-version amsterdam
//@[parse] compile-flags: --stop-after parsing --evm-version amsterdam

contract C {
    function functionIdentifier() external pure {
        assembly {
            function slotnum() {}
            //~[osaka]^ WARN: `slotnum` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[amsterdam]| ERROR: `slotnum` is reserved for a Yul builtin
        }
    }

    function localIdentifier() external pure {
        assembly {
            let slotnum := 1
            //~[osaka]^ WARN: `slotnum` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[amsterdam]| ERROR: `slotnum` is reserved for a Yul builtin
        }
    }

    function parameterIdentifier() external pure {
        assembly {
            function helper(slotnum) {}
            //~[osaka]^ WARN: `slotnum` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[amsterdam]| ERROR: `slotnum` is reserved for a Yul builtin
        }
    }

    function returnIdentifier() external pure {
        assembly {
            function helper() -> slotnum { slotnum := 1 }
            //~[osaka]^ WARN: `slotnum` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[amsterdam]| ERROR: `slotnum` is reserved for a Yul builtin
            //~[amsterdam]| ERROR: builtin function `slotnum` must be called
        }
    }
}
