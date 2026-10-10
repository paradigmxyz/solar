// ported-from: test/libsolidity/syntaxTests/types/encoding_fractional.sol
// ported-from: test/libsolidity/syntaxTests/types/encoding_fractional_abiencoderv2.sol
// ported-from: test/libsolidity/syntaxTests/types/encoding_packed_fractional.sol
// ported-from: test/libsolidity/syntaxTests/types/encoding_packed_fractional_abiencoderv2.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/583_abi_encode_packed_with_rational_number_constant.sol

contract Encode {
   function f1() public pure returns (bytes memory) {
       return abi.encode(0.1, 1); //~ ERROR: fractional numbers cannot be ABI-encoded
   }
}

contract EncodePacked {
   function f1() public pure returns (bytes memory) {
       return abi.encodePacked(0.1, 1); //~ ERROR: fractional numbers cannot be ABI-encoded
       //~^ ERROR: cannot perform packed encoding for a literal
   }
}

contract EncodePackedIntegralRational {
    function f() pure public { abi.encodePacked(0/1); } //~ ERROR: cannot perform packed encoding for a literal
}

contract EncodeVariants {
    function f() public view {
        abi.encodeWithSelector(0x12345678, 1 / 3); //~ ERROR: fractional numbers cannot be ABI-encoded
        abi.encodeWithSignature("g(uint256)", -0.5); //~ ERROR: fractional numbers cannot be ABI-encoded
        abi.encodeCall(this.g, (0.5)); //~ ERROR: mismatched types
        abi.encode(0.5 * 2);
    }
    function g(uint) external {}
}
