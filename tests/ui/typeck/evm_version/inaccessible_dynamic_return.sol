//@ revisions: homestead byzantium
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium

// Before Byzantium there is no `RETURNDATASIZE`, so the size of a dynamically encoded return
// value is unknowable: the call is legal as long as the value is discarded, and any use of it is
// rejected. From Byzantium on every use compiles.

struct S {
    uint256 a;
    bytes b;
}

interface I {
    function dyn() external returns (bytes memory);
    function dynStruct() external returns (S memory);
    function dynFixed() external returns (bytes[2] memory);
    function dynArray() external returns (uint256[] memory);
    function mixed() external returns (uint256, bytes memory);
}

library L {
    function dyn() external returns (bytes memory) {
        //~[byzantium]^ WARN: function state mutability can be restricted to pure
        return "x";
    }
}

contract C {
    function discarded(address a) external {
        I(a).dyn();
        I(a).dynArray();
        I(a).dynStruct();
        I(a).dynFixed();
        try I(a).dyn() {} catch {}
        L.dyn();
        (uint256 v, ) = I(a).mixed();
        v;
        function() external returns (bytes memory) pointer = I(a).dyn;
        pointer();
    }

    // A tuple expression statement discards every component, however deeply the tuples nest,
    // and a tuple assignment discards the components it drops.
    function discardedTuple(address a) external {
        (I(a).dyn(), I(a).dynArray());
        ((I(a).dyn(), I(a).dynStruct()), I(a).dyn());
        uint256 v;
        (v, ) = (v, I(a).dyn());
        (v, ) = I(a).mixed();
        for (uint256 i = 0; i < 1; I(a).dyn()) {
            i++;
        }
    }

    // A component that is used stays a use, whichever tuple it sits in.
    function usedInTuple(address a) external {
        (keccak256(I(a).dyn()), uint256(1));
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        bytes memory b;
        (b, ) = (I(a).dyn(), uint256(1));
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        b;
    }

    function assigned(address a) external {
        bytes memory b = I(a).dyn();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        b;
        uint256[] memory c = I(a).dynArray();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        c;
    }

    // A struct with a dynamic member and a fixed array of dynamic elements are dynamically
    // encoded too, through their component types.
    function aggregates(address a) external returns (uint256) {
        S memory s = I(a).dynStruct();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        bytes[2] memory f = I(a).dynFixed();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        f;
        return s.a;
    }

    function returned(address a) external returns (bytes memory) {
        return I(a).dyn();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
    }

    function argument(address a) external returns (bytes32) {
        return keccak256(I(a).dyn());
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
    }

    function member(address a) external {
        I(a).dyn().length;
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
    }

    function bound(address a) external {
        try I(a).dyn() returns (bytes memory b) {
            //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
            b;
        } catch {}
    }

    function boundMixed(address a) external {
        try I(a).mixed() returns (uint256 v, bytes memory b) {
            //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
            v;
            b;
        } catch {}
    }

    function pointer(address a) external {
        function() external returns (bytes memory) p = I(a).dyn;
        bytes memory b = p();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        b;
    }

    function library_() external {
        bytes memory b = L.dyn();
        //~[homestead]^ ERROR: cannot use the dynamically encoded return value of an external call
        b;
    }
}
