//@ run-call: Packed::checkScalars => 1
//@ run-call: Packed::checkStructSlots => 1
//@ run-call: Packed::checkStructCopyRoundTrip => 1
//@ run-call: Packed::checkMemoryToStorage => 1
//@ run-call: Packed::checkDynArray => 1
//@ run-call: Packed::checkFixedArray => 1
//@ run-call: Packed::checkSignedArray => 1
//@ run-call: Packed::checkDelete => 1
//@ run-call: Packed::checkGetters => 1

// Packed storage layout must match solc: value types share slots by byte
// size, packed low-to-high in declaration order; raw slot contents are
// asserted with assembly `sload`.

struct Market {
    uint128 totalSupplyAssets;
    uint128 totalSupplyShares;
    uint128 totalBorrowAssets;
    uint128 totalBorrowShares;
    uint128 lastUpdate;
    uint128 fee;
}

contract Packed {
    uint128 public a; // slot 0, offset 0
    uint64 public b; // slot 0, offset 16
    bool public c; // slot 0, offset 24
    int64 public d; // slot 1, offset 0 (does not fit after c)
    bytes8 public e; // slot 1, offset 8
    uint256 public f; // slot 2
    mapping(bytes32 => Market) public market; // slot 3
    uint128[] public arr; // slot 4
    uint64[5] public fixedArr; // slots 5..6
    int32[] public signedArr; // slot 7

    function raw(uint256 slot) internal view returns (uint256 out) {
        assembly {
            out := sload(slot)
        }
    }

    function checkScalars() external returns (uint256) {
        a = 0xAAAA;
        b = 0xBB;
        c = true;
        d = -2;
        e = 0x1122334455667788;
        f = 42;
        require(raw(0) == 0xAAAA | (uint256(0xBB) << 128) | (uint256(1) << 192), "slot0");
        uint256 dBits = uint256(uint64(int64(-2)));
        require(raw(1) == dBits | (uint256(0x1122334455667788) << 64), "slot1");
        require(raw(2) == 42, "slot2");
        require(a == 0xAAAA && b == 0xBB && c, "reads");
        require(d == -2, "signed read");
        require(e == bytes8(0x1122334455667788), "bytes read");
        return 1;
    }

    function marketBase(bytes32 id) internal pure returns (uint256) {
        return uint256(keccak256(abi.encode(id, uint256(3))));
    }

    function fillMarket(bytes32 id) internal {
        market[id].totalSupplyAssets = 1;
        market[id].totalSupplyShares = 2;
        market[id].totalBorrowAssets = 3;
        market[id].totalBorrowShares = 4;
        market[id].lastUpdate = 5;
        market[id].fee = 6;
    }

    function checkStructSlots() external returns (uint256) {
        bytes32 id = keccak256("id");
        fillMarket(id);
        uint256 base = marketBase(id);
        require(raw(base) == 1 | (uint256(2) << 128), "m slot0");
        require(raw(base + 1) == 3 | (uint256(4) << 128), "m slot1");
        require(raw(base + 2) == 5 | (uint256(6) << 128), "m slot2");
        require(raw(base + 3) == 0 && raw(base + 4) == 0 && raw(base + 5) == 0, "unpacked slots");
        return 1;
    }

    function checkStructCopyRoundTrip() external returns (uint256) {
        bytes32 id = keccak256("id");
        fillMarket(id);
        Market memory m = market[id];
        require(m.totalSupplyAssets == 1 && m.totalSupplyShares == 2, "copy 0");
        require(m.totalBorrowAssets == 3 && m.totalBorrowShares == 4, "copy 1");
        require(m.lastUpdate == 5 && m.fee == 6, "copy 2");
        return 1;
    }

    function checkMemoryToStorage() external returns (uint256) {
        bytes32 id = keccak256("id2");
        Market memory m = Market(7, 8, 9, 10, 11, 12);
        market[id] = m;
        uint256 base = marketBase(id);
        require(raw(base) == 7 | (uint256(8) << 128), "w slot0");
        require(raw(base + 1) == 9 | (uint256(10) << 128), "w slot1");
        require(raw(base + 2) == 11 | (uint256(12) << 128), "w slot2");
        require(market[id].fee == 12, "field read");
        return 1;
    }

    function checkDynArray() external returns (uint256) {
        arr.push(7);
        arr.push(9);
        arr.push(11);
        uint256 data = uint256(keccak256(abi.encode(uint256(4))));
        require(raw(4) == 3, "len");
        require(raw(data) == 7 | (uint256(9) << 128), "data0");
        require(raw(data + 1) == 11, "data1");
        require(arr[0] == 7 && arr[1] == 9 && arr[2] == 11, "elems");
        arr[1] = 13;
        require(raw(data) == 7 | (uint256(13) << 128), "elem write");
        arr.pop();
        require(raw(4) == 2 && raw(data + 1) == 0, "pop clears");
        return 1;
    }

    function checkFixedArray() external returns (uint256) {
        fixedArr = [uint64(1), 2, 3, 4, 5];
        require(
            raw(5) == 1 | (uint256(2) << 64) | (uint256(3) << 128) | (uint256(4) << 192),
            "fixed slot0"
        );
        require(raw(6) == 5, "fixed slot1");
        require(fixedArr[2] == 3 && fixedArr[4] == 5, "fixed reads");
        fixedArr[3] = 44;
        require(
            raw(5) == 1 | (uint256(2) << 64) | (uint256(3) << 128) | (uint256(44) << 192),
            "fixed write"
        );
        return 1;
    }

    function checkSignedArray() external returns (uint256) {
        signedArr.push(-5);
        signedArr.push(6);
        uint256 data = uint256(keccak256(abi.encode(uint256(7))));
        require(raw(data) == uint256(uint32(int32(-5))) | (uint256(6) << 32), "signed raw");
        require(signedArr[0] == -5 && signedArr[1] == 6, "signed reads");
        return 1;
    }

    function checkDelete() external returns (uint256) {
        bytes32 id = keccak256("id");
        fillMarket(id);
        delete market[id];
        uint256 base = marketBase(id);
        require(raw(base) == 0 && raw(base + 1) == 0 && raw(base + 2) == 0, "cleared");
        return 1;
    }

    function checkGetters() external returns (uint256) {
        bytes32 id = keccak256("id");
        fillMarket(id);
        a = 3;
        d = -9;
        (,,,, uint128 lastUpdate, uint128 fee) = this.market(id);
        require(lastUpdate == 5 && fee == 6, "struct getter");
        require(this.a() == 3, "scalar getter");
        require(this.d() == -9, "signed getter");
        return 1;
    }
}
