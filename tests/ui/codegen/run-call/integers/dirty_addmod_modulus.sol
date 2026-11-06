//@ codegen-matrix: standard
//@ run-call-fail: addmodDirtyModulus 1, 2 => Panic(0x12)
//@ run-call-fail: mulmodDirtyModulus 1, 2 => Panic(0x12)
//@ run-call: addmodCleanedModulus 13 => 0
//@ run-call: mulmodCleanedOperand 100 => 15

// Solidity `addmod`/`mulmod` convert their arguments to `uint256` before the
// zero-modulus check and the opcode, so a narrow local dirtied by inline
// assembly is cleaned first: a `uint8` holding `0x100` is zero, and one holding
// `0x107` is seven.
contract DirtyAddmodModulus {
    function addmodDirtyModulus(uint256 a, uint256 b) external pure returns (uint256) {
        uint8 modulus;
        assembly {
            modulus := 0x100
        }
        return addmod(a, b, modulus);
    }

    function mulmodDirtyModulus(uint256 a, uint256 b) external pure returns (uint256) {
        uint8 modulus;
        assembly {
            modulus := 0x100
        }
        return mulmod(a, b, modulus);
    }

    function addmodCleanedModulus(uint256 a) external pure returns (uint256) {
        uint8 modulus;
        assembly {
            modulus := 0x107
        }
        return addmod(a, 1, modulus);
    }

    function mulmodCleanedOperand(uint256 modulus) external pure returns (uint256) {
        uint8 operand;
        assembly {
            operand := 0x105
        }
        return mulmod(operand, 3, modulus);
    }
}
