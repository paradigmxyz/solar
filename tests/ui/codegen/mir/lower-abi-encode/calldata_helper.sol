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
    Item[] consideration;
    bytes signature;
}

struct Pair {
    Item[] items;
    uint256 tag;
}

interface Target {
    function a(Order[] calldata orders) external;
    function b(Order[] calldata orders, uint256 x) external;
    function c(Pair[] calldata pairs) external;
}

// Two sites encode `Order[]` and a third encodes `Pair[]`, which holds `Item[]`, and `Order[]`
// holds it twice. `-Osize` calls one helper per layout, and the `Order[]` helper calls the
// `Item[]` helper twice. `-Ogas` expands every site.
// SIZE-LABEL: fn @fa{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-LABEL: fn @fb{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-LABEL: fn @fc{{[( ]}}
// SIZE: icall @encode_abi_calldata.{{[0-9]+}}
// SIZE-COUNT-2: icall @encode_abi_calldata.{{[0-9]+}}
// GAS-LABEL: fn @fa{{[( ]}}
// GAS-NOT: encode_abi_calldata
// GAS-LABEL: fn @fb{{[( ]}}
// GAS-NOT: encode_abi_calldata
// GAS-LABEL: fn @fc{{[( ]}}
// GAS-NOT: encode_abi_calldata
// GAS-LABEL: fn @entry{{[( ]}}
contract CalldataHelper {
    function fa(Target t, Order[] calldata orders) external {
        t.a(orders);
    }

    function fb(Target t, Order[] calldata orders) external {
        t.b(orders, 1);
    }

    function fc(Target t, Pair[] calldata pairs) external {
        t.c(pairs);
    }
}
