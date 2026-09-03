//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: AssemblyConstantAccess::assemblyValues => 2, 0xabcd, 0x616263, true, 0x1212121212121212121212121212121212121212
//@ run-call: AssemblyConstantAccess::assemblyReferences => 0xabcd, 0x616263
//@ run-call: AssemblyConstantAccess::solidityValues => 0xabcd, 0x616263

contract AssemblyConstantAccess {
    uint256 constant integer = 2;
    bytes2 constant numeric = 0xabcd;
    bytes2 constant numericReference = numeric;
    bytes3 constant text = "abc";
    bytes3 constant textReference = text;
    bool constant boolean = true;
    address constant account = 0x1212121212121212121212121212121212121212;

    function assemblyValues()
        external
        pure
        returns (uint256 a, bytes2 b, bytes3 c, bool d, address e)
    {
        assembly {
            a := integer
            b := numeric
            c := text
            d := boolean
            e := account
        }
    }

    function assemblyReferences() external pure returns (bytes2 a, bytes3 b) {
        assembly {
            a := numericReference
            b := textReference
        }
    }

    function solidityValues() external pure returns (bytes2, bytes3) {
        return (numericReference, textReference);
    }
}
