// SPDX-License-Identifier: MIT
contract C {
    struct S {
        uint x;
        function(uint) internal f;
    }

    S public myVar; //~ ERROR: types containing non-public function pointers cannot be parameter or return types of public getter functions
}
