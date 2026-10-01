//@ compile-flags: -Zdataflow=storage
//@ filecheck:
// Storage-pointer aliasing cases collected from crytic/slither#515 and the issues it tracks.
// Each summary names the exact field, index, or mapping entry a function writes, including
// through storage pointers passed to and returned from internal and library functions.

// crytic/slither#70: a local storage pointer to a state struct.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:LocalPointer
// CHECK: sstore 0, arg0  ; write=slot(0)
// CHECK: summary: writes={slot(0)}
contract LocalPointer {
    struct Balance { uint balance; uint other; }
    Balance balances;
    function set(uint v) public {
        Balance storage other = balances;
        other.balance = v;
    }
}

// crytic/slither#82: a local reference to an inner mapping.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:InnerMapping
// CHECK: summary: writes={slot(0)[0][0]}
contract InnerMapping {
    mapping(uint128 => mapping(uint256 => uint256)) map;
    function init() external {
        mapping(uint256 => uint256) storage tmp = map[0];
        tmp[0] = 0;
    }
}

// crytic/slither#87: a pointer reassigned under a condition may write either struct, never
// the third.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:ConditionalPointer
// CHECK: summary: writes={slot(1), slot(0)}
contract ConditionalPointer {
    struct Balance { uint balance; }
    Balance balances1;
    Balance balances2;
    Balance balances3;
    function set(bool cond) external {
        Balance storage ref = balances1;
        if (cond) ref = balances2;
        ref.balance = 4;
    }
}

// crytic/slither#112: a storage pointer returned from a private function.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:ReturnedPointer
// CHECK: fn @increment:
// CHECK: summary: reads={slot(0), slot(2)} writes={slot(0), slot(2)}
// CHECK: fn @choose:
// CHECK: summary: ret0={slot(0), slot(2)}
contract ReturnedPointer {
    struct S { uint test; uint other; }
    S a;
    S b;
    function increment(bool useA) external {
        S storage s = choose(useA);
        s.test += 1;
    }
    function choose(bool useA) private view returns (S storage) {
        return useA ? a : b;
    }
}

// crytic/slither#270: a mapping entry passed as a storage parameter.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:MappingEntryParam
// CHECK: fn @f:
// CHECK: summary: writes={arg0}
// CHECK: fn @g:
// CHECK: icall @f, v2  ; write=slot(0)[caller]
contract MappingEntryParam {
    struct St { uint val; uint other; }
    mapping(address => St) map;
    function f(St storage s) internal { s.val = 10; }
    function g() external { f(map[msg.sender]); }
}

// crytic/slither#2598: a write through a `using for` library storage parameter.
library Roles {
    struct Role { mapping(address => bool) bearer; }
    function add(Role storage role, address account) internal {
        require(!has(role, account));
        role.bearer[account] = true;
    }
    function has(Role storage role, address account) internal view returns (bool) {
        return role.bearer[account];
    }
}

// CHECK-LABEL: dataflow storage (k=0): {{.*}}:MinterRole
// CHECK: fn @addMinter:
// CHECK: summary: reads={slot(1)[arg0]} writes={slot(1)[arg0]}
// CHECK: fn @add:
// CHECK: summary: reads={arg0[arg1]} writes={arg0[arg1]}
contract MinterRole {
    using Roles for Roles.Role;
    uint unrelated;
    Roles.Role minters;
    function addMinter(address a) public { _addMinter(a); }
    function _addMinter(address a) internal { minters.add(a); }
}

// crytic/slither#602: deleting one field through a storage parameter leaves its sibling
// distinct.
library GroupSelection {
    struct Storage { uint256[] tickets; uint256 tail; }
    function cleanup(Storage storage self) internal {
        delete self.tickets;
        self.tail = 0;
    }
}

// CHECK-LABEL: dataflow storage (k=0): {{.*}}:DeleteField
// CHECK: summary: reads={slot(0)} writes={slot(0), slot(1), data(slot(0))<*>}
// CHECK: summary: reads={arg0} writes={arg0, arg0.1, data(arg0)<*>}
contract DeleteField {
    using GroupSelection for GroupSelection.Storage;
    GroupSelection.Storage st;
    function cleanup() external { st.cleanup(); }
}

// crytic/slither#1286: a library over a fixed-size storage array.
library Setter {
    function set(uint256[1] storage self, uint256 key, uint256 value) internal {
        self[key] = value;
    }
}

// CHECK-LABEL: dataflow storage (k=0): {{.*}}:FixedArrayLibrary
// CHECK: fn @f:
// CHECK: summary: writes={slot(1)<arg0>}
// CHECK: fn @set:
// CHECK: summary: writes={arg0<arg1>}
contract FixedArrayLibrary {
    using Setter for uint256[1];
    uint256 before;
    uint256[1] params;
    function f(uint256 key, uint256 value) external { params.set(key, value); }
}

// crytic/slither#456: a nested member passed as a storage parameter.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:NestedMember
// CHECK: summary: reads={slot(0)[arg0].1} writes={slot(0)[arg0].1}
contract NestedMember {
    struct Limit { uint128 current; uint128 pending; }
    struct DailySpent { uint128 alreadySpent; uint64 periodEnd; }
    struct Config { Limit limit; DailySpent dailySpent; }
    mapping(address => Config) limits;
    function updateDailySpent(address wallet, uint128 amt) external {
        _update(limits[wallet].dailySpent, amt);
    }
    function _update(DailySpent storage ds, uint128 amt) internal { ds.alreadySpent += amt; }
}

// Implied by crytic/slither#515: a returned pointer to an array element.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:ReturnedElement
// CHECK: fn @get:
// CHECK: summary: reads={slot(0)} ret0=data(slot(0))<arg0 x2>
// CHECK: fn @set:
// CHECK: summary: reads={slot(0)} writes={data(slot(0))<arg0 x2>}
contract ReturnedElement {
    struct S { uint v; uint w; }
    S[] x;
    function get(uint i) internal view returns (S storage) { return x[i]; }
    function set(uint i, uint v) external { get(i).v = v; }
}

// Implied by crytic/slither#515: a nested mapping and struct path.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:NestedPath
// CHECK: summary: writes={slot(0)[arg0].1}
contract NestedPath {
    struct B { uint b; uint c; }
    struct A { B a; uint d; }
    mapping(address => A) m;
    function f(address k, uint v) external { m[k].a.c = v; }
}

// Implied by crytic/slither#515: deleting through a pointer and through a parameter.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:DeleteThroughPointer
// CHECK: fn @clear:
// CHECK: writes={slot(0)[arg0], slot(0)[arg0].1, data(slot(0)[arg0].1)<*>}
// CHECK: fn @clearViaParam:
// CHECK: summary: reads={slot(2)} writes={slot(2), data(slot(2))<*>}
contract DeleteThroughPointer {
    struct S { uint x; uint[] ys; }
    mapping(uint => S) m;
    S single;
    function clear(uint k) external {
        S storage p = m[k];
        delete p.x;
        delete p.ys;
    }
    function clearViaParam() external { _clear(single); }
    function _clear(S storage q) internal { delete q.ys; }
}

// crytic/slither#1742 for pointers: one callee returns each caller's own array.
// CHECK-LABEL: dataflow storage (k=0): {{.*}}:SharedCallee
// CHECK: fn @pick:
// CHECK: summary: ret0=arg0
// CHECK: fn @test1:
// CHECK: summary: reads={slot(0)} writes={slot(0), data(slot(0))<*>}
// CHECK: fn @test2:
// CHECK: summary: reads={slot(1)} writes={slot(1), data(slot(1))<*>}
contract SharedCallee {
    uint[] xs;
    uint[] ys;
    function pick(uint[] storage x) internal pure returns (uint[] storage) { return x; }
    function test1(uint v) public { pick(xs).push(v); }
    function test2(uint v) public { pick(ys).push(v); }
}
