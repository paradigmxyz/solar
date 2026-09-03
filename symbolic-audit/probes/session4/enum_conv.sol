contract EnumConv {
    enum E { A, B, C }
    enum Big { V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18, V19, V20, V21, V22, V23, V24, V25, V26, V27, V28, V29, V30, V31, V32, V33, V34, V35, V36, V37, V38, V39, V40, V41, V42, V43, V44, V45, V46, V47, V48, V49, V50, V51, V52, V53, V54, V55, V56, V57, V58, V59, V60, V61, V62, V63, V64, V65, V66, V67, V68, V69, V70, V71, V72, V73, V74, V75, V76, V77, V78, V79, V80, V81, V82, V83, V84, V85, V86, V87, V88, V89, V90, V91, V92, V93, V94, V95, V96, V97, V98, V99, V100, V101, V102, V103, V104, V105, V106, V107, V108, V109, V110, V111, V112, V113, V114, V115, V116, V117, V118, V119, V120, V121, V122, V123, V124, V125, V126, V127, V128, V129, V130, V131, V132, V133, V134, V135, V136, V137, V138, V139, V140, V141, V142, V143, V144, V145, V146, V147, V148, V149, V150, V151, V152, V153, V154, V155, V156, V157, V158, V159, V160, V161, V162, V163, V164, V165, V166, V167, V168, V169, V170, V171, V172, V173, V174, V175, V176, V177, V178, V179, V180, V181, V182, V183, V184, V185, V186, V187, V188, V189, V190, V191, V192, V193, V194, V195, V196, V197, V198, V199, V200, V201, V202, V203, V204, V205, V206, V207, V208, V209, V210, V211, V212, V213, V214, V215, V216, V217, V218, V219, V220, V221, V222, V223, V224, V225, V226, V227, V228, V229, V230, V231, V232, V233, V234, V235, V236, V237, V238, V239, V240, V241, V242, V243, V244, V245, V246, V247, V248, V249, V250, V251, V252, V253, V254, V255 }
    function fromU(uint256 v) external pure returns (E) { return E(v); }
    function fromU8(uint8 v) external pure returns (E) { return E(v); }
    function fromUidx(uint256 v) external pure returns (uint8) { return uint8(E(v)); }
    function toU(E e) external pure returns (uint256) { return uint256(e); }
    function toU8(E e) external pure returns (uint8) { return uint8(e); }
    function param(E e) external pure returns (E) { return e; }
    function paramArr(E[] calldata es, uint256 i) external pure returns (E) { return es[i]; }
    function paramArrMem(E[] memory es, uint256 i) external pure returns (E) { return es[i]; }
    function paramStatic(E[2] calldata es) external pure returns (E) { return es[1]; }
    function maxMin() external pure returns (E, E) { return (type(E).min, type(E).max); }
    function cmp(E x, E y) external pure returns (bool, bool, bool) { return (x < y, x == y, x >= y); }
    function big(uint256 v) external pure returns (Big) { return Big(v); }
    function bigMax() external pure returns (Big, uint8) { return (type(Big).max, uint8(type(Big).max)); }
    function bigParam(Big b) external pure returns (uint256) { return uint256(b); }
    function encode(E e) external pure returns (bytes memory) { return abi.encode(e); }
    function decode(bytes calldata d) external pure returns (E) { return abi.decode(d, (E)); }
    function decodeArr(bytes calldata d) external pure returns (E[] memory) { return abi.decode(d, (E[])); }
    function packed(E e) external pure returns (bytes memory) { return abi.encodePacked(e); }
    function packedArr(E[] calldata es) external pure returns (bytes memory) { return abi.encodePacked(es); }
    function ternary(bool c) external pure returns (E) { return c ? E.A : E.C; }
    function inLoop(uint256 n) external pure returns (uint256 s) { require(n < 3); for (E e = E.A; uint8(e) <= n; e = E(uint8(e) + 1)) s += uint256(e); }
    function inc(E e) external pure returns (E) { return E(uint8(e) + 1); }
    function switchLike(E e) external pure returns (uint256) { if (e == E.A) return 10; if (e == E.B) return 20; return 30; }
    function addrU160(uint160 v) external pure returns (address) { return address(v); }
    function addrU256(uint256 v) external pure returns (address) { return address(uint160(v)); }
    function u160Addr(address a) external pure returns (uint160) { return uint160(a); }
    function u256Addr(address a) external pure returns (uint256) { return uint256(uint160(a)); }
    function addrRound(address a) external pure returns (address) { return address(uint160(uint256(uint160(a)))); }
    function addrCmp(address a, address b) external pure returns (bool, bool) { return (a < b, a == b); }
    function addrZero(address a) external pure returns (bool) { return a == address(0); }
    function addrLit() external pure returns (address) { return 0x1234567890123456789012345678901234567890; }
    function addrFromLit() external pure returns (address) { return address(0xff); }
    function addrFromB20Lit() external pure returns (address) { return address(bytes20(hex"01")); }
    function truncU(uint256 v) external pure returns (uint8, uint16, uint32, uint64, uint128) { return (uint8(v), uint16(v), uint32(v), uint64(v), uint128(v)); }
    function truncI(int256 v) external pure returns (int8, int16, int32, int64, int128) { return (int8(v), int16(v), int32(v), int64(v), int128(v)); }
    function signFlip(uint8 v) external pure returns (int8, int16, int256) { return (int8(v), int16(uint16(v)), int256(uint256(v))); }
    function signFlipI(int8 v) external pure returns (uint8, uint16, uint256) { return (uint8(v), uint16(int16(v)), uint256(int256(v))); }
    function widenSigned(int8 v) external pure returns (int16, int256, uint256) { return (v, v, uint256(int256(v))); }
    function chain(uint256 v) external pure returns (uint256) { return uint256(uint8(uint16(uint32(v)))); }
    function chainI(int256 v) external pure returns (int256) { return int256(int8(int16(int32(v)))); }
    function chainMix(int256 v) external pure returns (uint256) { return uint256(uint8(int8(v))); }
    function chainMix2(uint256 v) external pure returns (int256) { return int256(int8(uint8(v))); }
    function boolRound(bool b) external pure returns (bool) { return !!b; }
    function boolOps(bool a, bool b) external pure returns (bool, bool, bool, bool) { return (a && b, a || b, a == b, a != b); }
    function boolToU(bool b) external pure returns (uint256) { return b ? 1 : 0; }
    function shortCircuit(bool a, uint256 x) external pure returns (bool) { return a || x / 0 == 0 ? true : false; }
    function shortCircuitAnd(bool a, uint256 x, uint256 y) external pure returns (bool) { return a && x / y == 0; }
    function u8FromLit() external pure returns (uint8, int8, uint16) { return (255, -128, 0xffff); }
    function i8Lit(int8 v) external pure returns (bool) { return v == -128; }
    function addrPayableConv(address a) external pure returns (address) { return address(payable(a)); }
    function udvtAddr(address a) external pure returns (uint160) { return uint160(bytes20(a)); }
    function enumStorageOOB(uint256 v) external returns (E) { E s = E(v); return s; }
    function enumFromDirty(uint256 raw) external pure returns (E r) { assembly { r := raw } }
    function enumFromDirtyCmp(uint256 raw) external pure returns (bool) { E r; assembly { r := raw } return r == E.C; }
    function enumFromDirtyToU(uint256 raw) external pure returns (uint256) { E r; assembly { r := raw } return uint256(r); }
    function enumFromDirtyEnc(uint256 raw) external pure returns (bytes memory) { E r; assembly { r := raw } return abi.encode(r); }
    function enumFromDirtyIdx(uint256 raw) external pure returns (uint256) { uint256[4] memory arr = [uint256(1), 2, 3, 4]; E r; assembly { r := raw } return arr[uint8(r)]; }
    function enumArrMem(uint256 v) external pure returns (E) { E[] memory es = new E[](2); es[1] = E(v); return es[1]; }
    function enumStructArr(uint8 v) external pure returns (E) { E[3] memory es = [E.A, E.B, E(v)]; return es[2]; }
}
