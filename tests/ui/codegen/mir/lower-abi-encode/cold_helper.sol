//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=mir
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=mir
//@[size] filecheck: --check-prefix=SIZE

struct Item {
    address token;
    uint256 amount;
}

struct Order {
    address maker;
    Item[] offer;
    bytes signature;
}

struct Fill {
    Item[] items;
    uint256 tag;
}

struct Claim {
    Item[] items;
    address owner;
}

error BadFills(Fill[] fills);
error BadClaims(Claim[] claims);

interface Target {
    function a(Order[] calldata orders) external;
    function b(Order[] calldata orders, uint256 x) external;
    function c(bytes memory payload) external;
    function d(Claim[] calldata claims) external;
}

// Two sites encode `Order[]` on a successful call, and two encode `Fill[]` only to revert.
// Two more encode one tuple on a successful call, and two encode another only to revert.
// A layout and a tuple that one successful path and two reverting paths share stay expanded.
// `-Osize` calls a helper for each layout and tuple. `-Ogas` expands what runs on success and
// shares what only reverts, which costs gas on a path that no successful call takes.
// SIZE-LABEL: fn @hotA{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-LABEL: fn @hotB{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-LABEL: fn @coldA{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-LABEL: fn @coldB{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-LABEL: fn @hotC{{[( ]}}
// SIZE: icall @encode_abi_tuple.{{[0-9]+}}
// SIZE-LABEL: fn @hotD{{[( ]}}
// SIZE: icall @encode_abi_tuple.{{[0-9]+}}
// SIZE-LABEL: fn @coldC{{[( ]}}
// SIZE: icall @encode_abi_tuple.{{[0-9]+}}
// SIZE-LABEL: fn @coldD{{[( ]}}
// SIZE: icall @encode_abi_tuple.{{[0-9]+}}
// GAS-LABEL: fn @hotA{{[( ]}}
// GAS-NOT: encode_abi_calldata
// GAS-LABEL: fn @hotB{{[( ]}}
// GAS-NOT: encode_abi_calldata
// GAS-LABEL: fn @coldA{{[( ]}}
// GAS: icall @encode_abi_calldata,
// GAS-LABEL: fn @coldB{{[( ]}}
// GAS: icall @encode_abi_calldata,
// GAS-LABEL: fn @hotC{{[( ]}}
// GAS-NOT: encode_abi_tuple
// GAS-LABEL: fn @hotD{{[( ]}}
// GAS-NOT: encode_abi_tuple
// GAS-LABEL: fn @coldC{{[( ]}}
// GAS: icall @encode_abi_tuple,
// GAS-LABEL: fn @coldD{{[( ]}}
// GAS: icall @encode_abi_tuple,
// GAS-LABEL: fn @mixedHot{{[( ]}}
// GAS-NOT: encode_abi_{{calldata|tuple}}
// GAS-LABEL: fn @mixedColdA{{[( ]}}
// GAS-NOT: encode_abi_{{calldata|tuple}}
// GAS-LABEL: fn @mixedColdB{{[( ]}}
// GAS-NOT: encode_abi_{{calldata|tuple}}
// GAS-LABEL: fn @decode_calldata_type{{[( ]}}
contract ColdHelper {
    function hotA(Target t, Order[] calldata orders) external {
        t.a(orders);
    }

    function hotB(Target t, Order[] calldata orders) external {
        t.b(orders, 1);
    }

    function coldA(Fill[] calldata fills) external pure {
        revert BadFills(fills);
    }

    function coldB(Fill[] calldata fills, uint256 x) external pure {
        if (x == 0) revert BadFills(fills);
    }

    function hotC(Target t, uint256 code, bytes memory data) external {
        t.c(abi.encode(code, data));
    }

    function hotD(Target t, uint256 code, bytes memory data) external {
        t.c(abi.encode(code + 1, data));
    }

    function coldC(uint256 code, bytes memory data) external pure {
        revert(string(abi.encode(data, code)));
    }

    function coldD(uint256 code, bytes memory data) external pure {
        revert(string(abi.encode(data, code + 1)));
    }

    function mixedHot(Target t, Claim[] calldata claims, uint256 code, bytes memory data) external {
        t.d(claims);
        t.c(abi.encode(code, data, code));
    }

    function mixedColdA(Claim[] calldata claims, uint256 code, bytes memory data)
        external
        pure
    {
        if (code == 0) revert BadClaims(claims);
        revert(string(abi.encode(code, data, code)));
    }

    function mixedColdB(Claim[] calldata claims, uint256 code, bytes memory data)
        external
        pure
    {
        if (code == 1) revert BadClaims(claims);
        revert(string(abi.encode(code, data, code)));
    }
}
