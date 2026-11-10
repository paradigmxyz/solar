//@[gas] run-call: retyped => 1
//@[size] run-call: retyped => 0
//@[mir] run-call: retyped => 0
//@[portable] run-call: retyped => 1
//@[gas] run-call: indirect => true
//@[size] run-call: indirect => false
//@[mir] run-call: indirect => false
//@[portable] run-call: indirect => true
//@ run-call: indirectGas => true
//@[gas] run-call: indirectSize => false
//@[size] run-call: indirectSize => false
//@[mir] run-call: indirectSize => false
//@[portable] run-call: indirectSize => true
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
    function retyped() external pure returns (uint256) {
        function() internal pure returns (bool) typed = Build.gasFirst;
        function() internal pure returns (uint256) raw;
        assembly { raw := typed }
        return raw();
    }

    function read(function() internal pure returns (bool) f) internal pure returns (bool) { return f(); }
    function indirect() external pure returns (bool) { function() internal pure returns (bool) f = Build.gasFirst; return f(); }
    /// @custom:solar-optimize gas
    function indirectGas() external pure returns (bool) { function() internal pure returns (bool) f = Build.gasFirst; return f(); }
    /// @custom:solar-optimize size
    function indirectSize() external pure returns (bool) { function() internal pure returns (bool) f = Build.gasFirst; return f(); }

    function objective() external pure returns(bool) { return Build.gasFirst(); }
    /// @custom:solar-optimize gas
    function gasFunction() external pure returns(bool) { return Build.gasFirst(); }
    /// @custom:solar-optimize size
    function sizeFunction() external pure returns(bool) { return Build.gasFirst(); }
}
