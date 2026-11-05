//@ codegen-matrix: standard
//@ filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Derived::check 0x0000000000000000000000000000000000000000 => false
//@ run-call: Derived::check 0x00000000000000000000000000000000deadbeef => true
//@ run-call: Derived::freeExternal => 3
//@ run-call: Derived::inherited => 30
//@ run-call: Derived::callExternal => 4
//@ run-call: Derived::ownPrivate => true
//@ run-call: Indirect::check 0x0000000000000000000000000000000000000000 => false
//@ run-call: Indirect::freeExternal => 3
//@ run-call: Indirect::inherited => 30
//@ run-call: Indirect::callExternal => 4

import {authorized, externalHelper, internalHelper, publicHelper} from "./auxiliary/inherited_function_visibility.sol";

contract Base {
    function authorized(address) private pure returns (bool) { return true; }
    function externalHelper() external pure returns (uint256) { return 4; }
    function internalHelper() internal pure returns (uint256) { return 10; }
    function publicHelper() public pure returns (uint256) { return 20; }

    function ownPrivate() public pure returns (bool) { return authorized(address(0)); }
}

// CHECK-LABEL: @module Derived
contract Derived is Base {
    // CHECK-LABEL: fn @check(
    // CHECK: icall @[[FREE_AUTH:authorized\.[0-9]+]], arg0
    function check(address caller) public pure returns (bool) { return authorized(caller); }
    // CHECK-LABEL: fn @freeExternal(
    // CHECK: icall @[[FREE_EXTERNAL:externalHelper\.[0-9]+]]
    function freeExternal() public pure returns (uint256) { return externalHelper(); }
    // CHECK-LABEL: fn @inherited(
    // CHECK: icall @internalHelper
    // CHECK: icall @publicHelper
    function inherited() public pure returns (uint256) { return internalHelper() + publicHelper(); }
    // CHECK-LABEL: fn @callExternal(
    // CHECK: staticcall
    function callExternal() public view returns (uint256) { return this.externalHelper(); }
}

// CHECK: fn @[[FREE_AUTH]](
// CHECK-DAG: 0xdeadbeef
// CHECK-DAG: = eq
// CHECK: fn @[[FREE_EXTERNAL]]()
// CHECK: ret 3

// CHECK-LABEL: @module Indirect
contract Indirect is Derived {}
