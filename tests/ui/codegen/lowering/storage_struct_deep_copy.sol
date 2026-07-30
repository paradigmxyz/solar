//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=SDC

// Copying a memory struct with dynamic fields (bytes/string/dynamic arrays)
// into storage — via assignment or a struct-element array push — writes each
// field's storage length and payload, not the memory pointer word. Verified
// behaviorally against solc, including struct-element push and pop.

contract StorageStructDeepCopy {
    struct Sel {
        address addr;
        bytes4[] selectors;
    }

    Sel single;
    Sel[] list;

    // SDC-LABEL: fn @assign
    // The dynamic array field's length and elements are written to storage.
    // SDC: sstore {{v[0-9]+}}, {{v[0-9]+}}
    // SDC: sstore 1,
    function assign(address a, bytes4 x) public {
        bytes4[] memory ss = new bytes4[](1);
        ss[0] = x;
        single = Sel(a, ss);
    }

    // SDC-LABEL: fn @push
    // A struct element occupies multiple slots; its dynamic field deep-copies.
    // SDC: keccak256
    // SDC: sstore
    function push(address a, bytes4 x) public {
        bytes4[] memory ss = new bytes4[](1);
        ss[0] = x;
        list.push(Sel(a, ss));
    }

    // SDC-LABEL: fn @pop
    // SDC: sstore
    function pop() public {
        list.pop();
    }
}
