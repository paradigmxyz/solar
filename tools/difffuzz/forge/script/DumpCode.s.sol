// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";

/// Deploys each Seaport build at the same address, one at a time from the same state, and writes
/// its runtime code to code/<build>.hex for forge equivalence: the immutables, such as the domain
/// separator, which hashes the address, then match.
contract DumpCode is Script {
    function run() public {
        string[4] memory names = ["solc-optimized", "solc-reference", "solar-optimized", "solar-reference"];
        string[4] memory paths = [
            "lib/seaport/optimized-out/Consideration.sol/Consideration.json",
            "lib/seaport/reference-out/ReferenceConsideration.sol/ReferenceConsideration.json",
            "lib/seaport/solar-optimized-out/Consideration.sol/Consideration.json",
            "lib/seaport/solar-reference-out/ReferenceConsideration.sol/ReferenceConsideration.json"
        ];
        address controller = deployCode("lib/seaport/optimized-out/ConduitController.sol/ConduitController.json");
        for (uint256 i; i < 4; i++) {
            uint256 snap = vm.snapshot();
            address seaport = deployCode(paths[i], abi.encode(controller));
            vm.writeFile(string.concat("code/", names[i], ".hex"), vm.toString(seaport.code));
            vm.writeFile(string.concat("code/", names[i], ".address"), vm.toString(seaport));
            vm.revertTo(snap);
        }
    }
}
