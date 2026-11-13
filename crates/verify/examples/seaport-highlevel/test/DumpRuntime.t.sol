// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { SeaportVariants } from "./Deploy.sol";

/// Writes each variant's runtime code to equiv/<variant>-tstore.hex for
/// scripts/equiv.sh.
contract DumpRuntime is SeaportVariants {
    function test_dumpRuntime() public {
        _deployController();
        (address a, bytes memory opt) = _deployAt(OPTIMIZED);
        (address b, bytes memory ref) = _deployAt(REFERENCE);
        (address c, bytes memory hl) = _deployAt(HIGHLEVEL);
        assertEq(a, b);
        assertEq(a, c);
        vm.writeFile("equiv/optimized-tstore.hex", vm.toString(opt));
        vm.writeFile("equiv/reference-tstore.hex", vm.toString(ref));
        vm.writeFile("equiv/highlevel-tstore.hex", vm.toString(hl));
    }
}
