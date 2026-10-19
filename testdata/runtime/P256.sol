// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {P256} from "contracts/utils/cryptography/P256.sol";

contract P256Verifier {
    function verify(bytes32 h, bytes32 r, bytes32 s, bytes32 qx, bytes32 qy) external view returns (bool) {
        return P256.verifySolidity(h, r, s, qx, qy);
    }
}
