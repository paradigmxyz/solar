//@ revisions: none gas
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@ compile-flags: --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@[none] filecheck: --check-prefix=NONE
//@[gas] filecheck: --check-prefix=GAS
//@ normalize-stdout-test: "(?s).+" -> ""

// A read that takes no operands, such as a calldata parameter or `msg.sender`, is re-executed at
// each use like an immediate, so it takes the origins of the use. The origin of its definition
// would send the debugger back to the parameter list or to an earlier statement at every use.

// The loop condition reads `n` again on every iteration, under the condition and not the function.
// NONE-LABEL: invoke=loop@
// NONE: [loop] !metadata(
// NONE-NEXT: push 4 !metadata(span=[[COND:[0-9]+..[0-9]+]])
// NONE-NEXT: calldataload !metadata(span=[[COND]])
// NONE-NEXT: dup 2 !metadata(span=[[COND]])
// NONE-NEXT: lt !metadata(span=[[COND]])

// An unoptimized build keeps the definition of a read that is an item of its own, such as the
// initializer of a local, because the read may be the only instruction that anchors the item.
// NONE-LABEL: invoke=sender@
// NONE: caller !metadata(span=[[#START:]]..
// NONE-SAME: [[#START+10]])

// The global value numbering shares `msg.sender` between the statements, so the emit reads it
// again under the emit and not under the `require`.
// GAS-LABEL: invoke=transfer@
// GAS: caller !metadata(span=[[REQUIRE:[0-9]+..[0-9]+]])
// GAS-NOT: caller !metadata(span=[[REQUIRE]])
// GAS: caller !metadata(span=[[EMIT:[0-9]+..[0-9]+]])
// GAS: log3 !metadata(span=[[EMIT]])
contract C {
    mapping(address => uint256) balanceOf;

    event Transfer(address indexed from, address indexed to, uint256 amount);

    function loop(uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
    }

    function sender() external view returns (uint256 r) {
        address me = msg.sender;
        r = uint160(me) + 1;
    }

    function transfer(address to, uint256 amount) external {
        require(balanceOf[msg.sender] >= amount);
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        emit Transfer(msg.sender, to, amount);
    }
}
