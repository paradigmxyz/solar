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

// A parameter read takes the origin of its use even when it has a single use, as the definition of
// a parameter is the whole function. The use is the `emit` in the inlined body of `emitE`, which
// the definition does not enclose, and the function header would be the stop in the middle of it.

// CHECK-LABEL: invoke=fromLibrary@
// CHECK: calldatacopy !metadata(
// CHECK-NEXT: push 64 !metadata(span=[[INLINED:[0-9]+..[0-9]+]])
// CHECK-NEXT: dup 4 !metadata(span=[[INLINED]])
// CHECK-NEXT: add !metadata(span=[[INLINED]])
// CHECK-NEXT: push 4 !metadata(span=[[INLINED]])
// CHECK-NEXT: calldataload !metadata(span=[[INLINED]])

// A single use in another block than the definition is a copy that moved there, and takes the
// origin of its use too. The read of `msg.sender` runs in the loop body, under the call and not
// under the declaration of `s` before the loop.

// CHECK: caller !metadata(span=[[LOOP:[0-9]+..[0-9]+]])
// CHECK: sload !metadata(span=[[LOOP]])
interface I {
    event E(uint64 a, bytes b);
}

library L {
    function emitE(uint64 a, bytes memory b) internal {
        emit I.E(a, b);
    }
}

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

contract C is I {
    function fromLibrary(uint64 a, bytes calldata b) external {
        L.emitE(a, b);
    }

    // Shares the encoding of the `emit` in `emitE`, so the code of `fromLibrary` is the shared one.
    function direct(uint64 a, bytes calldata b) external {
        emit I.E(a, b);
    }
}

contract D {
    mapping(address => uint256) b;
    uint256 n;

    function _add(address to, uint256 v) internal {
        b[to] += v;
    }

    function run(uint256 k) external {
        address s = msg.sender;
        for (uint256 i = 0; i < k; i++) {
            _add(s, i);
        }
        n = k;
    }
}
