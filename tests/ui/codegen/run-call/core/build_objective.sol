//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gas] run-call: objective => true
//@[size] run-call: objective => false
//@[mir] run-call: objective => false
//@[portable] run-call: objective => true
//@ run-call: gasFunction => true
//@[gas] run-call: sizeFunction => false
//@[size] run-call: sizeFunction => false
//@[mir] run-call: sizeFunction => false
//@[portable] run-call: sizeFunction => true
import {Build} from "solar:core/Build.sol";
contract Test {
    function objective() external pure returns(bool) { return Build.gasFirst(); }
    /// @custom:solar-optimize gas
    function gasFunction() external pure returns(bool) { return Build.gasFirst(); }
    /// @custom:solar-optimize size
    function sizeFunction() external pure returns(bool) { return Build.gasFirst(); }
}
