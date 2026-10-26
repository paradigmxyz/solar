{
  // Opcode-level gas attribution tracer for debug_traceTransaction (geth JS tracer API, also run by
  // anvil through revm-inspectors). Every unit of execution gas lands on exactly one step; call and
  // create steps get only their own overhead because the callee's steps are attributed separately.
  init: false,
  names: null,
  pops: null,
  pushes: null,
  baseKind: null,
  frames: [],
  ops: {},
  kinds: {},
  ko: {},
  dyn: {},
  pcs: {},
  red: {},
  codes: {},
  blk: 0,
  steps: 0,
  exec: 0,
  refund: 0,
  firstGas: -1,

  setup: function () {
    var n = [], po = [], pu = [], k = [];
    for (var i = 0; i < 256; i++) { n.push("0x" + i.toString(16)); po.push(0); pu.push(0); k.push("other"); }
    var t = function (op, name, p, q, kind) { n[op] = name; po[op] = p; pu[op] = q; k[op] = kind; };
    t(0x00, "STOP", 0, 0, "halt");
    var ar = ["ADD", "MUL", "SUB", "DIV", "SDIV", "MOD", "SMOD"];
    for (i = 0; i < ar.length; i++) t(0x01 + i, ar[i], 2, 1, "arith");
    t(0x08, "ADDMOD", 3, 1, "arith"); t(0x09, "MULMOD", 3, 1, "arith");
    t(0x0a, "EXP", 2, 1, "arith"); t(0x0b, "SIGNEXTEND", 2, 1, "arith");
    var cm = ["LT", "GT", "SLT", "SGT", "EQ"];
    for (i = 0; i < cm.length; i++) t(0x10 + i, cm[i], 2, 1, "compare");
    t(0x15, "ISZERO", 1, 1, "compare");
    t(0x16, "AND", 2, 1, "bitwise"); t(0x17, "OR", 2, 1, "bitwise"); t(0x18, "XOR", 2, 1, "bitwise");
    t(0x19, "NOT", 1, 1, "bitwise"); t(0x1a, "BYTE", 2, 1, "bitwise"); t(0x1b, "SHL", 2, 1, "bitwise");
    t(0x1c, "SHR", 2, 1, "bitwise"); t(0x1d, "SAR", 2, 1, "bitwise"); t(0x1e, "CLZ", 1, 1, "bitwise");
    t(0x20, "KECCAK256", 2, 1, "keccak");
    var env0 = { 0x30: "ADDRESS", 0x32: "ORIGIN", 0x33: "CALLER", 0x34: "CALLVALUE", 0x3a: "GASPRICE",
      0x41: "COINBASE", 0x42: "TIMESTAMP", 0x43: "NUMBER", 0x44: "PREVRANDAO", 0x45: "GASLIMIT",
      0x46: "CHAINID", 0x47: "SELFBALANCE", 0x48: "BASEFEE", 0x4a: "BLOBBASEFEE", 0x58: "PC",
      0x5a: "GAS", 0x38: "CODESIZE" };
    for (var key in env0) t(Number(key), env0[key], 0, 1, "env");
    t(0x31, "BALANCE", 1, 1, "env"); t(0x3b, "EXTCODESIZE", 1, 1, "env"); t(0x3f, "EXTCODEHASH", 1, 1, "env");
    t(0x40, "BLOCKHASH", 1, 1, "env"); t(0x49, "BLOBHASH", 1, 1, "env");
    t(0x3c, "EXTCODECOPY", 4, 0, "env"); t(0x39, "CODECOPY", 3, 0, "env");
    t(0x35, "CALLDATALOAD", 1, 1, "calldata"); t(0x36, "CALLDATASIZE", 0, 1, "calldata");
    t(0x37, "CALLDATACOPY", 3, 0, "calldata");
    t(0x3d, "RETURNDATASIZE", 0, 1, "returndata"); t(0x3e, "RETURNDATACOPY", 3, 0, "returndata");
    t(0x50, "POP", 1, 0, "stack");
    t(0x51, "MLOAD", 1, 1, "memory"); t(0x52, "MSTORE", 2, 0, "memory"); t(0x53, "MSTORE8", 2, 0, "memory");
    t(0x59, "MSIZE", 0, 1, "memory"); t(0x5e, "MCOPY", 3, 0, "memory");
    t(0x54, "SLOAD", 1, 1, "storage"); t(0x55, "SSTORE", 2, 0, "storage");
    t(0x5c, "TLOAD", 1, 1, "transient"); t(0x5d, "TSTORE", 2, 0, "transient");
    t(0x56, "JUMP", 1, 0, "control"); t(0x57, "JUMPI", 2, 0, "control"); t(0x5b, "JUMPDEST", 0, 0, "control");
    t(0x5f, "PUSH0", 0, 1, "push");
    for (i = 1; i <= 32; i++) t(0x5f + i, "PUSH" + i, 0, 1, "push");
    for (i = 1; i <= 16; i++) { t(0x7f + i, "DUP" + i, i, i + 1, "stack"); t(0x8f + i, "SWAP" + i, i + 1, i + 1, "stack"); }
    for (i = 0; i <= 4; i++) t(0xa0 + i, "LOG" + i, 2 + i, 0, "log");
    t(0xf0, "CREATE", 3, 1, "call"); t(0xf1, "CALL", 7, 1, "call"); t(0xf2, "CALLCODE", 7, 1, "call");
    t(0xf3, "RETURN", 2, 0, "halt"); t(0xf4, "DELEGATECALL", 6, 1, "call"); t(0xf5, "CREATE2", 4, 1, "call");
    t(0xfa, "STATICCALL", 6, 1, "call"); t(0xfd, "REVERT", 2, 0, "halt"); t(0xfe, "INVALID", 0, 0, "halt");
    t(0xff, "SELFDESTRUCT", 1, 0, "halt");
    this.names = n; this.pops = po; this.pushes = pu; this.baseKind = k;
    this.init = true;
  },

  num: function (v) {
    return v > 0xffffffffffffn ? -1 : Number(v);
  },

  memCost: function (bytes) {
    var w = Math.floor(bytes / 32);
    return 3 * w + Math.floor(w * w / 512);
  },

  add: function (map, key, v) {
    map[key] = (map[key] || 0) + v;
  },

  addDyn: function (comp, op, v) {
    if (v === 0) return;
    var m = this.dyn[comp];
    if (!m) m = this.dyn[comp] = {};
    m[op] = (m[op] || 0) + v;
  },

  hex: function (a) {
    return toHex(a);
  },

  // Static analysis: does every path from `pc` reach REVERT or INVALID without a conditional branch?
  // Follows fallthrough and `PUSH target JUMP`.
  revertish: function (info, pc) {
    if (info.memo[pc] !== undefined) return info.memo[pc];
    info.memo[pc] = false;
    var code = info.code, len = info.len, p = pc, res = false, prevPush = -1;
    while (p < len) {
      var op = code[p];
      if (op === 0xfd || op === 0xfe) { res = true; break; }
      if (op === 0x00 || op === 0xf3 || op === 0x57 || op === 0xff) break;
      if (op === 0x56) { if (prevPush >= 0) res = this.revertish(info, prevPush); break; }
      if (op >= 0x60 && op <= 0x7f) {
        var nb = op - 0x5f, v = 0;
        for (var j = 1; j <= nb && j <= 6; j++) v = v * 256 + (p + j < len ? code[p + j] : 0);
        prevPush = nb <= 6 ? v : -1;
        p += nb + 1;
        continue;
      }
      prevPush = op === 0x5f ? 0 : -1;
      p += 1;
    }
    info.memo[pc] = res;
    return res;
  },

  codeInfo: function (addr, db) {
    var info = this.codes[addr];
    if (info) return info;
    var code = [], raw = null;
    try { raw = db.getCode(toAddress(addr)); } catch (e) { raw = null; }
    var len = raw ? raw.length : 0;
    for (var i = 0; i < len; i++) code.push(raw[i]);
    info = this.codes[addr] = { code: code, len: len, memo: {} };
    return info;
  },

  frame: function (log, db, depth) {
    var f = this.frames[depth];
    if (f) return f;
    var addr = this.pendingTo !== null && this.pendingTo !== undefined ? this.pendingTo : this.hex(log.contract.getAddress());
    this.pendingTo = null;
    var store = this.hex(log.contract.getAddress());
    if (!this.pcs[addr]) this.pcs[addr] = {};
    f = { addr: addr, store: store, pm: this.pcs[addr], info: null, sh: [], pend: null,
      unread: {}, nUnread: 0, lm: {}, lmBlk: -1, childUsed: 0, childSteps: 0, sawChild: false };
    this.frames[depth] = f;
    return f;
  },

  reattr: function (rec, kind) {
    if (rec.k === kind) return;
    if (rec.done) {
      this.kinds[rec.k] -= rec.g; this.add(this.kinds, kind, rec.g);
      var name = this.names[rec.op];
      this.ko[rec.k + "|" + name] -= rec.g; this.add(this.ko, kind + "|" + name, rec.g);
      if (rec.k !== this.baseKind[rec.op]) rec.pe[2] -= rec.g;
    }
    rec.k = kind;
    if (rec.done && kind !== this.baseKind[rec.op]) rec.pe[2] += rec.g;
  },

  guardChain: function (ent, depth) {
    if (!ent || depth > 6) return;
    var r = ent.r, op = r.op;
    if ((op >= 0x5f && op <= 0x7f)) { this.reattr(r, "guard"); return; }
    // Comparisons, and the AND/OR that merge several checks into one branch.
    if (((op >= 0x10 && op <= 0x15) || op === 0x16 || op === 0x17) && r.k !== "guard") {
      this.reattr(r, "guard");
      if (r.ins) for (var i = 0; i < r.ins.length; i++) this.guardChain(r.ins[i], depth + 1);
    }
  },

  fmpChain: function (ent) {
    if (!ent) return;
    var r = ent.r;
    if (r.op >= 0x5f && r.op <= 0x7f) { this.reattr(r, "fmp"); return; }
    if (r.op === 0x01 && r.k !== "fmp") {
      this.reattr(r, "fmp");
      if (r.ins) for (var i = 0; i < r.ins.length; i++) {
        var q = r.ins[i].r;
        if (q.op >= 0x5f && q.op <= 0x7f) this.reattr(q, "fmp");
      }
    }
  },

  redAdd: function (key, g) {
    var e = this.red[key];
    if (!e) e = this.red[key] = [0, 0];
    e[0] += 1; e[1] += g;
  },

  // Unread-store bookkeeping. `unread[off] = rec` for 32-byte MSTOREs not yet read.
  memRead: function (f, off, size) {
    if (f.nUnread === 0 || size <= 0 || off < 0) return;
    for (var k in f.unread) {
      var o = Number(k);
      if (o + 32 > off && o < off + size) { delete f.unread[k]; f.nUnread--; }
    }
  },

  memWrite: function (f, off, size, exactStore) {
    if (f.nUnread === 0 || size <= 0 || off < 0) return;
    for (var k in f.unread) {
      var o = Number(k);
      if (o + 32 > off && o < off + size) {
        var r = f.unread[k];
        delete f.unread[k]; f.nUnread--;
        if (o >= off && o + 32 <= off + size) this.redAdd(r.fmp ? "dead_store_overwritten_fmp" : "dead_store_overwritten", r.g);
      }
    }
  },

  blkMem: function (f) {
    if (f.lmBlk !== this.blk) { f.lm = {}; f.lmBlk = this.blk; }
    return f.lm;
  },

  lmInvalidate: function (f, off, size) {
    var lm = this.blkMem(f);
    for (var k in lm) {
      var o = Number(k);
      if (o + 32 > off && o < off + size) delete lm[k];
    }
  },

  finalize: function (f, rec, memAfter, gasAfter) {
    var op = rec.op, name = this.names[op], cost = rec.cost;
    var isCall = op === 0xf1 || op === 0xf2 || op === 0xf4 || op === 0xfa || op === 0xf0 || op === 0xf5;
    var memexp = 0, childGas = 0;
    if (isCall) {
      if (gasAfter >= 0) {
        // A child with steps gets its gas from its own steps; a precompile's gas stays here.
        cost = rec.gas - gasAfter - (f.sawChild && f.childSteps > 0 ? f.childUsed : 0);
      } else {
        cost = rec.gas;
      }
      if (f.sawChild && f.childSteps === 0) childGas = f.childUsed;
    }
    if (memAfter >= 0) {
      if (memAfter > rec.mem) memexp = this.memCost(memAfter) - this.memCost(rec.mem);
    } else if (op === 0xf3 || op === 0xfd) {
      memexp = cost;
    }
    if (memexp > cost) memexp = cost;
    var rest = cost - memexp, cold = 0, extra = 0, extraKind = null, callDyn = 0;
    if (op === 0x37 || op === 0x39 || op === 0x3e || op === 0x5e || op === 0x3c) {
      extra = 3 * Math.ceil(rec.a / 32); extraKind = "copy_words";
      if (op === 0x3c && rest - extra >= 2600) cold = 2500;
    } else if (op === 0x20) {
      extra = 6 * Math.ceil(rec.a / 32); extraKind = "keccak_words";
    } else if (op >= 0xa0 && op <= 0xa4) {
      extra = 8 * rec.a; extraKind = "log_data";
    } else if (op === 0x0a) {
      extra = rest - 10; extraKind = "exp_bytes";
    } else if (op === 0x54) {
      if (rest >= 2100) cold = 2000;
    } else if (op === 0x55) {
      if (rest === 2200 || rest === 5000 || rest === 22100) cold = 2100;
      extra = rest - cold - 100; extraKind = extra >= 19900 ? "sstore_set" : extra > 0 ? "sstore_reset" : null;
    } else if (op === 0x31 || op === 0x3b || op === 0x3f) {
      if (rest >= 2600) cold = 2500;
    } else if (isCall) {
      var base = op === 0xf0 || op === 0xf5 ? 32000 : 100;
      var r = rest - childGas - base;
      if (rec.a > 0 && r >= 6700) { this.addDyn("call_value", name, 6700); r -= 6700; callDyn += 6700; }
      if (r >= 25000 && base === 100) { this.addDyn("new_account", name, 25000); r -= 25000; callDyn += 25000; }
      if (r >= 2500 && base === 100) { cold = 2500; r -= 2500; }
      if (r > 0) { this.addDyn("call_other", name, r); callDyn += r; }
      if (childGas > 0) { this.addDyn("precompile", name, childGas); callDyn += childGas; }
    }
    if (extra < 0) extra = 0;
    if (extraKind) this.addDyn(extraKind, name, extra);
    if (cold) this.addDyn("cold_access", name, cold);
    if (memexp) { this.addDyn("mem_expansion", name, memexp); this.add(this.kinds, "mem_expansion", memexp); }
    var o = this.ops[name];
    if (!o) o = this.ops[name] = [0, 0, 0];
    o[0] += 1; o[1] += cost; o[2] += cost - memexp - cold - extra - callDyn;
    rec.g = cost - memexp - childGas;
    if (childGas) this.add(this.kinds, "precompile", childGas);
    rec.done = true;
    this.add(this.kinds, rec.k, rec.g);
    this.add(this.ko, rec.k + "|" + name, rec.g);
    rec.pe[1] += cost;
    if (rec.k !== this.baseKind[op]) rec.pe[2] += rec.g;
    this.exec += cost;
  },

  step: function (log, db) {
    if (!this.init) this.setup();
    var depth = log.getDepth();
    var gas = log.getGas();
    if (this.firstGas < 0) this.firstGas = gas;
    // Frames deeper than this one have ended.
    for (var d = this.frames.length - 1; d > depth; d--) {
      var g = this.frames[d];
      if (g) {
        if (g.pend) { this.finalize(g, g.pend, -1, -1); g.pend = null; }
        for (var uk in g.unread) this.redAdd(g.unread[uk].fmp ? "unread_at_exit_fmp" : "unread_at_exit", g.unread[uk].g);
      }
      this.frames.length = d;
    }
    var f = this.frame(log, db, depth);
    var mem = log.memory.length();
    if (f.pend) { this.finalize(f, f.pend, mem, gas); f.pend = null; }
    this.steps++;
    this.refund = log.getRefund();
    var op = log.op.toNumber(), pc = log.getPC();
    var pe = f.pm[pc];
    if (!pe) pe = f.pm[pc] = [0, 0, 0];
    pe[0] += 1;
    var rec = { op: op, k: this.baseKind[op], cost: log.getCost(), gas: gas, mem: mem, a: 0, pe: pe, g: 0, done: false, ins: null };
    f.childUsed = 0; f.childSteps = 0; f.sawChild = false;
    var st = log.stack, sh = f.sh, npop = this.pops[op], npush = this.pushes[op], i;

    // Dynamic arguments that size dynamic gas.
    if (op === 0x37 || op === 0x39 || op === 0x3e || op === 0x5e) rec.a = this.num(st.peek(2));
    else if (op === 0x3c) rec.a = this.num(st.peek(3));
    else if (op === 0x20 || (op >= 0xa0 && op <= 0xa4)) rec.a = this.num(st.peek(1));
    else if (op === 0xf1 || op === 0xf2) rec.a = st.peek(2) > 0n ? 1 : 0;

    // Block boundaries.
    if (op === 0x5b) {
      this.blk++;
      for (i = 0; i < sh.length; i++) sh[i].c = false;
    }

    // Memory traffic classification.
    if (op === 0x51 || op === 0x52 || op === 0x53) {
      var off = this.num(st.peek(0));
      if (off === 0x40 && op !== 0x53) {
        rec.k = "fmp";
        this.fmpChain(sh[sh.length - 1]);
        if (op === 0x52) this.fmpChain(sh[sh.length - 2]);
      }
      var lm = this.blkMem(f);
      if (op === 0x51) {
        this.memRead(f, off, 32);
        var s = lm[off];
        if (s === 1) this.redAdd(off === 0x40 ? "load_after_store_same_block_fmp" : "load_after_store_same_block", 3);
        else if (s === 2) this.redAdd(off === 0x40 ? "repeated_load_same_block_fmp" : "repeated_load_same_block", 3);
        if (off >= 0) lm[off] = s === 1 ? 1 : 2;
      } else if (off >= 0) {
        var size = op === 0x52 ? 32 : 1;
        this.memWrite(f, off, size, op === 0x52);
        if (op === 0x53) this.memRead(f, off, 1);
        this.lmInvalidate(f, off, size);
        if (op === 0x52) {
          lm[off] = 1;
          var ur = { g: 3, fmp: off === 0x40 };
          f.unread[off] = ur; f.nUnread++;
        }
      }
    } else if (op === 0x20 || op === 0xf3 || op === 0xfd || (op >= 0xa0 && op <= 0xa4)) {
      this.memRead(f, this.num(st.peek(0)), this.num(st.peek(1)));
    } else if (op === 0x37 || op === 0x39 || op === 0x3e) {
      var o1 = this.num(st.peek(0)), s1 = this.num(st.peek(2));
      this.memWrite(f, o1, s1, false); this.lmInvalidate(f, o1, s1);
    } else if (op === 0x3c) {
      var o2 = this.num(st.peek(1)), s2 = this.num(st.peek(3));
      this.memWrite(f, o2, s2, false); this.lmInvalidate(f, o2, s2);
    } else if (op === 0x5e) {
      var s3 = this.num(st.peek(2)), d3 = this.num(st.peek(0));
      this.memRead(f, this.num(st.peek(1)), s3);
      this.memWrite(f, d3, s3, false); this.lmInvalidate(f, d3, s3);
    } else if (op === 0xf1 || op === 0xf2) {
      this.memRead(f, this.num(st.peek(3)), this.num(st.peek(4)));
      var o4 = this.num(st.peek(5)), s4 = this.num(st.peek(6));
      this.memWrite(f, o4, s4, false);
    } else if (op === 0xf4 || op === 0xfa) {
      this.memRead(f, this.num(st.peek(2)), this.num(st.peek(3)));
      var o5 = this.num(st.peek(4)), s5 = this.num(st.peek(5));
      this.memWrite(f, o5, s5, false);
    } else if (op === 0xf0 || op === 0xf5) {
      this.memRead(f, this.num(st.peek(1)), this.num(st.peek(2)));
    } else if (op === 0x54 || op === 0x55) {
      var key = f.store + ":" + st.peek(0).toString(16);
      if (!this.slots) this.slots = {};
      var prev = this.slots[key];
      if (op === 0x54) {
        if (prev !== undefined && prev[0] === this.blk) this.redAdd(prev[1] === 1 ? "sload_after_sstore_same_block" : "repeated_sload_same_block", 100);
        this.slots[key] = [this.blk, prev !== undefined && prev[0] === this.blk && prev[1] === 1 ? 1 : 2];
      } else {
        this.slots[key] = [this.blk, 1];
      }
    }

    // Jumps: guard detection.
    if (op === 0x57) {
      if (!f.info) f.info = this.codeInfo(f.addr, db);
      var dest = this.num(st.peek(0));
      if (f.info.len > 0 && ((dest >= 0 && this.revertish(f.info, dest)) || this.revertish(f.info, pc + 1))) {
        rec.k = "guard";
        this.guardChain(sh[sh.length - 1], 0);
        this.guardChain(sh[sh.length - 2], 0);
      }
    }

    // Shadow stack update.
    if (op >= 0x80 && op <= 0x8f) {
      var src = sh[sh.length - (op - 0x7f)];
      sh.push({ r: rec, c: src ? src.c : false });
    } else if (op >= 0x90 && op <= 0x9f) {
      var n = op - 0x8f, top = sh.length - 1, other = top - n;
      if (other >= 0) { var tmp = sh[top]; sh[top] = sh[other]; sh[other] = tmp; }
    } else {
      var ins = npop > 0 ? sh.splice(Math.max(0, sh.length - npop), npop) : null;
      var pure = (op >= 0x01 && op <= 0x0b) || (op >= 0x10 && op <= 0x1e);
      if (pure) {
        var allConst = ins !== null && ins.length === npop;
        for (i = 0; allConst && i < ins.length; i++) if (!ins[i].c) allConst = false;
        if (allConst) rec.k = "recipe";
        rec.ins = ins;
        sh.push({ r: rec, c: allConst });
      } else if (npush > 0) {
        sh.push({ r: rec, c: op >= 0x5f && op <= 0x7f });
      }
    }
    if (op === 0x56 || op === 0x57) {
      this.blk++;
      for (i = 0; i < sh.length; i++) sh[i].c = false;
    }
    f.pend = rec;
  },

  enter: function (frame) {
    this.blk++;
    this.pendingTo = this.hex(frame.getTo());
    this.enterDepth = (this.enterDepth || 0) + 1;
  },

  exit: function (res) {
    this.blk++;
    // The parent frame is the deepest frame that is not the exiting child.
    var parentDepth = this.enterDepth;
    this.enterDepth -= 1;
    var p = this.frames[parentDepth];
    var child = this.frames[parentDepth + 1];
    if (p) {
      p.sawChild = true;
      p.childUsed = res.getGasUsed();
      p.childSteps = child ? 1 : 0;
    }
    if (child) {
      if (child.pend) { this.finalize(child, child.pend, -1, -1); child.pend = null; }
      for (var uk in child.unread) this.redAdd(child.unread[uk].fmp ? "unread_at_exit_fmp" : "unread_at_exit", child.unread[uk].g);
      this.frames.length = parentDepth + 1;
    }
    this.pendingTo = null;
  },

  fault: function (log, db) {},

  result: function (ctx, db) {
    for (var d = this.frames.length - 1; d >= 1; d--) {
      var f = this.frames[d];
      if (!f) continue;
      if (f.pend) { this.finalize(f, f.pend, -1, -1); f.pend = null; }
      for (var uk in f.unread) this.redAdd(f.unread[uk].fmp ? "unread_at_exit_fmp" : "unread_at_exit", f.unread[uk].g);
    }
    return { ops: this.ops, kinds: this.kinds, ko: this.ko, dyn: this.dyn, red: this.red, pcs: this.pcs,
      steps: this.steps, exec: this.exec, refund: this.refund, firstGas: this.firstGas,
      gasUsed: ctx.gasUsed, intrinsic: ctx.intrinsicGas, gasLimit: ctx.gas, to: toHex(ctx.to) };
  }
}
