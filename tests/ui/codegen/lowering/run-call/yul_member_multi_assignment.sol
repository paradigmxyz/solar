//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: array [8, 7, 6] => 2, 7, 6
//@ run-call: mixedBytes 0x010203 => 9, 2, 0x0203
//@ run-call: mixedString "hello" => 5, "ell"
//@ run-call: returned [8, 7, 6] => [7, 6]
//@ run-call: reversed [8, 7, 6] => 2, 7
//@ run-call: slot => 14
//@ run-call: functionPointer => 0x0000000000000000000000000000000000001234, 0xabcdef01

contract YulMemberMultiAssignment {
    uint256[] values;

    // CHECK-LABEL: fn @array(
    // CHECK: [[RES:v[0-9]+]] = icall @shift
    // CHECK-NEXT: [[OFF:v[0-9]+]] = extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, arg0, 0
    // CHECK-NEXT: [[TMP:v[0-9]+]] = make_calldata_slice [[PTR]], [[LEN]]
    // CHECK-NEXT: [[KEEP:v[0-9]+]] = extract_value calldataslice, [[TMP]], 1
    // CHECK-NEXT: make_calldata_slice [[OFF]], [[KEEP]]
    function array(uint256[] calldata a) external pure returns (uint256, uint256, uint256) {
        assembly {
            function shift(o, l) -> x, y {
                x := add(o, 0x20)
                y := sub(l, 1)
            }
            a.offset, a.length := shift(a.offset, a.length)
        }
        return (a.length, a[0], a[1]);
    }

    // CHECK-LABEL: fn @mixedBytes(
    // CHECK: [[RES:v[0-9]+]] = icall @shift
    // CHECK-NEXT: extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: [[OFF:v[0-9]+]] = extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value [[TY]], [[RES]], 2
    // CHECK-NEXT: extract_value [[TY]], [[RES]], 3
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, arg0, 0
    // CHECK-NEXT: [[TMP:v[0-9]+]] = make_calldata_slice [[PTR]], [[LEN]]
    // CHECK-NEXT: [[KEEP:v[0-9]+]] = extract_value calldataslice, [[TMP]], 1
    // CHECK-NEXT: make_calldata_slice [[OFF]], [[KEEP]]
    function mixedBytes(bytes calldata b) external pure returns (uint256, uint256, bytes memory) {
        uint256 z;
        uint256 l;
        assembly {
            function shift(o, n) -> x, y, w, v {
                x := 9
                y := add(o, 1)
                w := sub(n, 1)
                v := w
            }
            z, b.offset, b.length, l := shift(b.offset, b.length)
        }
        return (z, l, b);
    }

    // CHECK-LABEL: fn @mixedString(
    // CHECK: [[RES:v[0-9]+]] = icall @trim
    // CHECK-NEXT: [[OFF:v[0-9]+]] = extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value [[TY]], [[RES]], 2
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, arg0, 0
    // CHECK-NEXT: [[TMP:v[0-9]+]] = make_calldata_slice [[PTR]], [[LEN]]
    // CHECK-NEXT: [[KEEP:v[0-9]+]] = extract_value calldataslice, [[TMP]], 1
    // CHECK-NEXT: make_calldata_slice [[OFF]], [[KEEP]]
    function mixedString(string calldata s) external pure returns (uint256, string memory) {
        uint256 n;
        assembly {
            function trim(o, l) -> x, y, z {
                x := add(o, 1)
                y := l
                z := sub(l, 2)
            }
            s.offset, n, s.length := trim(s.offset, s.length)
        }
        return (n, s);
    }

    // CHECK-LABEL: fn @returned(
    // CHECK: [[RES:v[0-9]+]] = icall @shift
    // CHECK-NEXT: [[OFF:v[0-9]+]] = extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: [[EMPTY:v[0-9]+]] = make_calldata_slice 0, 0
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, [[EMPTY]], 0
    // CHECK-NEXT: [[TMP:v[0-9]+]] = make_calldata_slice [[PTR]], [[LEN]]
    // CHECK-NEXT: [[KEEP:v[0-9]+]] = extract_value calldataslice, [[TMP]], 1
    // CHECK-NEXT: [[OUT:v[0-9]+]] = make_calldata_slice [[OFF]], [[KEEP]]
    // CHECK-NEXT: ret [[OUT]]
    function returned(uint256[] calldata a) external pure returns (uint256[] calldata r) {
        assembly {
            function shift(o, l) -> x, y {
                x := add(o, 0x20)
                y := sub(l, 1)
            }
            r.offset, r.length := shift(a.offset, a.length)
        }
    }

    // CHECK-LABEL: fn @reversed(
    // CHECK: [[RES:v[0-9]+]] = icall @shift
    // CHECK-NEXT: [[LEN:v[0-9]+]] = extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: [[OFF:v[0-9]+]] = extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: [[KEEP:v[0-9]+]] = extract_value calldataslice, arg0, 1
    // CHECK-NEXT: [[TMP:v[0-9]+]] = make_calldata_slice [[OFF]], [[KEEP]]
    // CHECK-NEXT: [[PTR:v[0-9]+]] = extract_value calldataslice, [[TMP]], 0
    // CHECK-NEXT: make_calldata_slice [[PTR]], [[LEN]]
    function reversed(uint256[] calldata a) external pure returns (uint256, uint256) {
        assembly {
            function shift(o, l) -> y, x {
                x := add(o, 0x20)
                y := sub(l, 1)
            }
            a.length, a.offset := shift(a.offset, a.length)
        }
        return (a.length, a[0]);
    }

    // CHECK-LABEL: fn @slot(
    // CHECK: [[RES:v[0-9]+]] = icall @get{{.*}}, 0
    // CHECK-NEXT: extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: [[SLOT:v[0-9]+]] = extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: sload [[SLOT]]
    function slot() external returns (uint256) {
        values.push(5);
        uint256[] storage p;
        uint256 q;
        assembly {
            function get(s) -> x, y {
                x := 9
                y := s
            }
            q, p.slot := get(values.slot)
        }
        return p[0] + q;
    }

    // CHECK-LABEL: fn @functionPointer(
    // CHECK: [[RES:v[0-9]+]] = icall @get
    // CHECK-NEXT: [[ADDR:v[0-9]+]] = extract_value [[TY:struct[0-9]+]], [[RES]], 0
    // CHECK-NEXT: [[SEL:v[0-9]+]] = extract_value [[TY]], [[RES]], 1
    // CHECK-NEXT: and [[SEL]], 0xffffffff
    // CHECK: and [[ADDR]], 0xffffffffffffffffffffffffffffffffffffffff
    function functionPointer() external view returns (address, bytes4) {
        function() external returns (uint256) f = this.slot;
        assembly {
            function get() -> a, s {
                a := 0x1234
                s := 0xabcdef01
            }
            f.address, f.selector := get()
        }
        return (f.address, f.selector);
    }
}
