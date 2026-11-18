//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ compile-flags: --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A read without operands that has a single use runs once, so it keeps the origin of its
// definition when it moves, as LLVM keeps the `DebugLoc` of an instruction that moves within its
// block. The inliner puts the use of `msg.sender` and `msg.value` in the body of `_mint`, but the
// reads stay on the call's arguments, and the call keeps instructions to map to.

// CHECK-LABEL: invoke=deposit@
// CHECK: caller !metadata(span=[[#SENDER:]]..
// CHECK-SAME: [[#SENDER+10]])
// CHECK-NOT: caller
// CHECK: callvalue !metadata(span=[[#SENDER+12]]..
// CHECK-SAME: [[#SENDER+21]])

// A read with several uses is a copy at each use, so it takes the origin of its use. The value
// numbering shares `msg.sender` between the statements, and the emit reads it again under the emit
// and not under the `require`.

// CHECK-LABEL: invoke=shared@
// CHECK: caller !metadata(span=[[REQUIRE:[0-9]+..[0-9]+]])
// CHECK-NOT: caller !metadata(span=[[REQUIRE]])
// CHECK: caller !metadata(span=[[EMIT:[0-9]+..[0-9]+]])
// CHECK: log2 !metadata(span=[[EMIT]])
contract W {
    mapping(address => uint256) b;

    event E(address indexed a, uint256 v);

    function _mint(address to, uint256 v) internal {
        b[to] += v;
    }

    function deposit() external payable {
        _mint(msg.sender, msg.value);
    }

    function shared(uint256 v) external {
        require(b[msg.sender] >= v);
        b[msg.sender] -= v;
        emit E(msg.sender, v);
    }
}
