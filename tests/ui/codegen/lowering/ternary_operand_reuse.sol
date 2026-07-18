//@compile-flags: -Zcodegen --emit=bin-runtime

contract TernaryOperandReuse {
    // `caller()` is not rematerializable: consuming it as the modulus and then
    // returning it used to make the backend lose the value and ICE.
    function addCaller(uint256 x, uint256 y) external view returns (uint256, address) {
        assembly {
            let sender := caller()
            let result := addmod(x, y, sender)
            mstore(0, result)
            mstore(32, sender)
            return(0, 64)
        }
    }

    // Repeated ternary operands must keep one physical copy beyond the two
    // occurrences consumed by `MULMOD`.
    function mulRepeated(uint256 modulus) external view returns (uint256, address) {
        assembly {
            let sender := caller()
            let result := mulmod(sender, sender, modulus)
            mstore(0, result)
            mstore(32, sender)
            return(0, 64)
        }
    }
}
