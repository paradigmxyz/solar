//@ filecheck:
//@ codegen-matrix: standard
//@ run-call: StructMappingParam::getM 1, 5 => 16
//@ run-call: StructMappingParam::getM 2, 5 => 0
//@ run-call: MappingArrayParam::m 1, 0, 1 => 2
//@ run-call: ArrayParam::len => 2
//@ run-call: ArrayParam::values 0 => 7
//@ run-call: ArrayParam::values 1 => 8
//@ run-call: BytesParam::len => 2
//@ run-call: BytesParam::at 0 => 97
//@ run-call: BytesParam::at 1 => 98
//@ run-call: ForwardingParam::getM 3, 5 => 16
//@ run-call: ForwardingParam::getM 3, 6 => 36
//@ run-call: TernaryParam::getM 1, 5; constructor=[true] => 16
//@ run-call: TernaryParam::getM 2, 5; constructor=[true] => 0
//@ run-call: TernaryParam::getM 2, 5; constructor=[false] => 16
// ported-from: test/libsolidity/semanticTests/types/struct_mapping_abstract_constructor_param.sol
// ported-from: test/libsolidity/semanticTests/types/array_mapping_abstract_constructor_param.sol

struct S {
    mapping(uint256 => uint256) m;
}

abstract contract StructMappingBase {
    constructor(S storage s) {
        s.m[5] = 16;
    }
}

// A storage-reference argument passes the referenced slot, so the base
// constructor body indexes the derived contract's mapping entry directly.
// CHECK-LABEL: @module StructMappingParam
// CHECK: fn @constructor()
// CHECK: [[ENTRY:v[0-9]+]] = mapping_slot 1, 0
// CHECK-NEXT: [[SLOT:v[0-9]+]] = mapping_slot 5, [[ENTRY]]
// CHECK-NEXT: sstore [[SLOT]], 16
contract StructMappingParam is StructMappingBase {
    mapping(uint256 => S) m;

    constructor() StructMappingBase(m[1]) {}

    function getM(uint256 a, uint256 b) external view returns (uint256) {
        return m[a].m[b];
    }
}

abstract contract MappingArrayBase {
    constructor(mapping(uint256 => uint256)[] storage m) {
        m.push();
        m[0][1] = 2;
    }
}

contract MappingArrayParam is MappingArrayBase {
    mapping(uint256 => mapping(uint256 => uint256)[]) public m;

    constructor() MappingArrayBase(m[1]) {}
}

abstract contract ArrayBase {
    constructor(uint256[] storage a) {
        a.push(7);
        a.push(8);
    }
}

contract ArrayParam is ArrayBase {
    uint256[] public values;

    constructor() ArrayBase(values) {}

    function len() external view returns (uint256) {
        return values.length;
    }
}

abstract contract BytesBase {
    constructor(bytes storage b) {
        b.push("a");
        b.push("b");
    }
}

contract BytesParam is BytesBase {
    bytes data;

    constructor() BytesBase(data) {}

    function len() external view returns (uint256) {
        return data.length;
    }

    function at(uint256 i) external view returns (uint8) {
        return uint8(data[i]);
    }
}

// The middle constructor's own storage parameter forwards to its base.
abstract contract ForwardingMiddle is StructMappingBase {
    constructor(S storage s) StructMappingBase(s) {
        s.m[6] = 36;
    }
}

// Both levels see the same slot, so the forwarded parameter loads once.
// CHECK-LABEL: @module ForwardingParam
// CHECK: fn @constructor()
// CHECK: [[ENTRY:v[0-9]+]] = mapping_slot 3, 0
// CHECK-NEXT: [[BASE:v[0-9]+]] = mapping_slot 5, [[ENTRY]]
// CHECK-NEXT: sstore [[BASE]], 16
// CHECK: [[MIDDLE:v[0-9]+]] = mapping_slot 6, [[ENTRY]]
// CHECK-NEXT: sstore [[MIDDLE]], 36
contract ForwardingParam is ForwardingMiddle {
    mapping(uint256 => S) m;

    constructor() ForwardingMiddle(m[3]) {}

    function getM(uint256 a, uint256 b) external view returns (uint256) {
        return m[a].m[b];
    }
}

// A runtime-selected reference merges both slots before the base body runs.
// CHECK-LABEL: @module TernaryParam
// CHECK: fn @constructor(arg0: bool)
// CHECK: [[ELSE:v[0-9]+]] = mapping_slot 2, 0
// CHECK: [[THEN:v[0-9]+]] = mapping_slot 1, 0
// CHECK: [[ENTRY:v[0-9]+]] = phi [bb{{[0-9]+}}: [[THEN]]], [bb{{[0-9]+}}: [[ELSE]]]
// CHECK-NEXT: [[SLOT:v[0-9]+]] = mapping_slot 5, [[ENTRY]]
// CHECK-NEXT: sstore [[SLOT]], 16
contract TernaryParam is StructMappingBase {
    mapping(uint256 => S) m;

    constructor(bool flag) StructMappingBase(flag ? m[1] : m[2]) {}

    function getM(uint256 a, uint256 b) external view returns (uint256) {
        return m[a].m[b];
    }
}
