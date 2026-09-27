//@ revisions: osaka amsterdam
//@[osaka] compile-flags: --evm-version osaka
//@[amsterdam] compile-flags: --evm-version amsterdam

contract C {
    function functionIdentifier() external pure {
        assembly {
            function slotnum() {}
            //~[osaka]^ WARN: `slotnum` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[amsterdam]| ERROR: expected identifier, found Yul EVM builtin keyword `slotnum`
        }
    }

    function localIdentifier() external pure {
        assembly {
            let slotnum := 1
            //~[osaka]^ WARN: `slotnum` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[amsterdam]| ERROR: expected identifier, found Yul EVM builtin keyword `slotnum`
        }
    }
}
