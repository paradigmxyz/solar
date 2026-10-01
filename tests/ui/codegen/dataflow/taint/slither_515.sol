//@ revisions: summary context
//@[summary] compile-flags: -Zdataflow=taint
//@[context] compile-flags: -Zdataflow=taint -Zdataflow-k=1
//@[summary] filecheck: --check-prefix=SUMMARY
//@[context] filecheck: --check-prefix=CONTEXT
// Data-dependence cases from crytic/slither#515 and the issues it tracks.

// crytic/slither#1742: a shared internal callee keeps each caller's own dependencies.
// SUMMARY-LABEL: dataflow taint (k=0): {{.*}}:SharedIdentity
// SUMMARY: fn @f:
// SUMMARY: summary: ret0={arg0}
// SUMMARY: fn @test1:
// SUMMARY: summary: slot(0)<-{arg0}
// SUMMARY: fn @test2:
// SUMMARY: summary: slot(1)<-{arg0, calldatasize}
// CONTEXT-LABEL: dataflow taint (k=1): {{.*}}:SharedIdentity
// CONTEXT: fn @f [@test1]({arg0}):
// CONTEXT: fn @f [@test2]({arg0, calldatasize}):
contract SharedIdentity {
    uint a;
    uint b;
    function f(uint x) internal pure returns (uint) { return x; }
    function test1(uint paramA) public { a = f(paramA); }
    function test2(uint paramB) public { b = f(paramB + msg.data.length); }
}

// crytic/slither#2288: a mapping entry read depends on its key.
// SUMMARY-LABEL: dataflow taint (k=0): {{.*}}:KeyDependency
// SUMMARY: v3 = sload v2  ; taint={caller, sload(slot(0)[caller])}
contract KeyDependency {
    mapping(address => uint) mappingVar;
    uint ref;
    function set() external { ref = mappingVar[msg.sender]; }
}

// crytic/slither#1436: a timestamp stored in one field does not reach the array length.
// SUMMARY-LABEL: dataflow taint (k=0): {{.*}}:FieldTaint
// SUMMARY: fn @createPost:
// SUMMARY: summary: slot(0)<-{sload(slot(0))}
// SUMMARY-SAME: data(slot(0))<* x3>.1<-{caller, timestamp, sload(slot(0))}
// SUMMARY: fn @likePost:
// SUMMARY-NOT: timestamp
// SUMMARY: summary: ret0={arg0, sload(slot(0))}
contract FieldTaint {
    struct Post { uint256 id; uint256 timeCreated; address contributor; }
    Post[] posts;
    function createPost() external {
        posts.push(Post(posts.length, block.timestamp, msg.sender));
    }
    function likePost(uint256 postIdx) external view returns (bool) {
        return postIdx < posts.length;
    }
}
