//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: AbiEncodePackedExternalFunction::matches() => true
//@[gas] run-call: AbiEncodePackedExternalFunction::matches() => true
//@[size] run-call: AbiEncodePackedExternalFunction::matches() => true

contract AbiEncodePackedExternalFunction {
    function target() external {}

    function matches() external view returns (bool) {
        bytes32 pointerHash = keccak256(abi.encodePacked(this.target));
        bytes32 partsHash = keccak256(abi.encodePacked(address(this), this.target.selector));
        return pointerHash == partsHash;
    }
}
