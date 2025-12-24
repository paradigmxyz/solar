// SPDX-License-Identifier: MIT
contract C {
    // Internal function type as public state variable - should error
    function(uint) internal public myInternalFunc; //~ ERROR: types containing non-public function pointers cannot be parameter or return types of public getter functions

    // External function type as public state variable - should be OK
    function(uint) external public myExternalFunc;

    // Internal function type as internal state variable - should be OK (no getter)
    function(uint) internal internal myInternalFuncInternal;
}
