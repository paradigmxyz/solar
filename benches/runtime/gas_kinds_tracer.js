{
  // Gas-by-kind tracer for `debug_traceTransaction` (geth JS tracer API, also run by anvil through
  // revm-inspectors). benchmark.py passes this source as the `tracer` option.
  //
  // Every unit of execution gas lands in exactly one kind, so the kinds sum to `exec`. Call and
  // create steps carry only their own overhead: the callee's steps are attributed separately, and a
  // callee without steps (a precompile) is charged to `precompile`. Memory expansion is split off
  // every step into `mem_expansion`.
  //
  // Kinds, by opcode: stack (DUP/SWAP/POP), push, control (JUMP/JUMPI/JUMPDEST), arith, compare,
  // bitwise, memory, storage, transient, calldata, returndata, keccak, call, log, env, halt
  // (STOP/RETURN/REVERT/INVALID/SELFDESTRUCT), other (opcodes the tracer does not know). Three kinds
  // reattribute steps by their role:
  //   recipe: a pure op whose inputs are all constants pushed in the same basic block, such as
  //     `PUSH0 NOT`; the constant pushes keep their `push` kind.
  //   guard: a JUMPI where either successor reaches REVERT or INVALID without another conditional
  //     branch, its target push, and the comparison/AND/OR chain feeding its condition.
  //   fmp: MLOAD/MSTORE at offset 0x40, the push of that offset, and the bump ADD with its operand
  //     pushes.
  init: false,
  pops: null,
  pushes: null,
  baseKind: null,
  frames: [],
  codes: {},
  kinds: {},
  exec: 0,
  refund: 0,
  firstGas: -1,
  pendingTo: null,
  enterExec: [],

  setup: function () {
    var po = [], pu = [], k = [], i;
    for (i = 0; i < 256; i++) { po.push(0); pu.push(0); k.push("other"); }
    var t = function (op, p, q, kind) { po[op] = p; pu[op] = q; k[op] = kind; };
    t(0x00, 0, 0, "halt");
    for (i = 0x01; i <= 0x07; i++) t(i, 2, 1, "arith");
    t(0x08, 3, 1, "arith"); t(0x09, 3, 1, "arith"); t(0x0a, 2, 1, "arith"); t(0x0b, 2, 1, "arith");
    for (i = 0x10; i <= 0x14; i++) t(i, 2, 1, "compare");
    t(0x15, 1, 1, "compare");
    for (i = 0x16; i <= 0x18; i++) t(i, 2, 1, "bitwise");
    t(0x19, 1, 1, "bitwise");
    for (i = 0x1a; i <= 0x1d; i++) t(i, 2, 1, "bitwise");
    t(0x1e, 1, 1, "bitwise");
    t(0x20, 2, 1, "keccak");
    var env0 = [0x30, 0x32, 0x33, 0x34, 0x38, 0x3a, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
      0x4a, 0x58, 0x5a];
    for (i = 0; i < env0.length; i++) t(env0[i], 0, 1, "env");
    var env1 = [0x31, 0x3b, 0x3f, 0x40, 0x49];
    for (i = 0; i < env1.length; i++) t(env1[i], 1, 1, "env");
    t(0x39, 3, 0, "env"); t(0x3c, 4, 0, "env");
    t(0x35, 1, 1, "calldata"); t(0x36, 0, 1, "calldata"); t(0x37, 3, 0, "calldata");
    t(0x3d, 0, 1, "returndata"); t(0x3e, 3, 0, "returndata");
    t(0x50, 1, 0, "stack");
    t(0x51, 1, 1, "memory"); t(0x52, 2, 0, "memory"); t(0x53, 2, 0, "memory");
    t(0x59, 0, 1, "memory"); t(0x5e, 3, 0, "memory");
    t(0x54, 1, 1, "storage"); t(0x55, 2, 0, "storage");
    t(0x5c, 1, 1, "transient"); t(0x5d, 2, 0, "transient");
    t(0x56, 1, 0, "control"); t(0x57, 2, 0, "control"); t(0x5b, 0, 0, "control");
    for (i = 0; i <= 32; i++) t(0x5f + i, 0, 1, "push");
    for (i = 1; i <= 16; i++) { t(0x7f + i, i, i + 1, "stack"); t(0x8f + i, i + 1, i + 1, "stack"); }
    for (i = 0; i <= 4; i++) t(0xa0 + i, 2 + i, 0, "log");
    t(0xf0, 3, 1, "call"); t(0xf1, 7, 1, "call"); t(0xf2, 7, 1, "call"); t(0xf4, 6, 1, "call");
    t(0xf5, 4, 1, "call"); t(0xfa, 6, 1, "call");
    t(0xf3, 2, 0, "halt"); t(0xfd, 2, 0, "halt"); t(0xfe, 0, 0, "halt"); t(0xff, 1, 0, "halt");
    this.pops = po; this.pushes = pu; this.baseKind = k;
    this.init = true;
  },

  num: function (v) {
    return v > 0xffffffffffffn ? -1 : Number(v);
  },

  memCost: function (bytes) {
    var w = Math.floor(bytes / 32);
    return 3 * w + Math.floor(w * w / 512);
  },

  add: function (key, v) {
    this.kinds[key] = (this.kinds[key] || 0) + v;
  },

  isPush: function (op) {
    return op >= 0x5f && op <= 0x7f;
  },

  // Static analysis: does every path from `pc` reach REVERT or INVALID without a conditional
  // branch? Follows fallthrough and `PUSH target JUMP`.
  revertish: function (info, pc) {
    if (info.memo[pc] !== undefined) return info.memo[pc];
    info.memo[pc] = false;
    var code = info.code, len = info.len, p = pc, res = false, prevPush = -1;
    while (p < len) {
      var op = code[p];
      if (op === 0xfd || op === 0xfe) { res = true; break; }
      if (op === 0x00 || op === 0xf3 || op === 0x57 || op === 0xff) break;
      if (op === 0x56) { if (prevPush >= 0) res = this.revertish(info, prevPush); break; }
      if (op > 0x5f && op <= 0x7f) {
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

  frame: function (log, depth) {
    var f = this.frames[depth];
    if (f) return f;
    var addr = this.pendingTo !== null ? this.pendingTo : toHex(log.contract.getAddress());
    this.pendingTo = null;
    f = { addr: addr, info: null, sh: [], pend: null, childUsed: 0, childHadSteps: false };
    this.frames[depth] = f;
    return f;
  },

  // Moves a step to another kind, also after its gas was counted.
  reattr: function (rec, kind) {
    if (rec.k === kind) return;
    if (rec.done) { this.add(rec.k, -rec.g); this.add(kind, rec.g); }
    rec.k = kind;
  },

  guardChain: function (ent, depth) {
    if (!ent || depth > 6) return;
    var r = ent.r, op = r.op;
    if (this.isPush(op)) { this.reattr(r, "guard"); return; }
    // Comparisons, and the AND/OR that merge several checks into one branch.
    if (((op >= 0x10 && op <= 0x15) || op === 0x16 || op === 0x17) && r.k !== "guard") {
      this.reattr(r, "guard");
      if (r.ins) for (var i = 0; i < r.ins.length; i++) this.guardChain(r.ins[i], depth + 1);
    }
  },

  fmpChain: function (ent) {
    if (!ent) return;
    var r = ent.r;
    if (this.isPush(r.op)) { this.reattr(r, "fmp"); return; }
    if (r.op === 0x01 && r.k !== "fmp") {
      this.reattr(r, "fmp");
      if (r.ins) for (var i = 0; i < r.ins.length; i++) {
        var q = r.ins[i].r;
        if (this.isPush(q.op)) this.reattr(q, "fmp");
      }
    }
  },

  // Counts the gas of a finished step. `memAfter` is the frame's memory size after the step (-1 if
  // the frame ended); `gasAfter` is the frame's gas after a call step returned (-1 if unknown).
  finalize: function (f, rec, memAfter, gasAfter) {
    var op = rec.op, cost = rec.cost, memexp = 0, childGas = 0;
    if (this.baseKind[op] === "call") {
      // A callee with steps gets its gas from its own steps; a precompile's gas stays here.
      cost = gasAfter >= 0 ? rec.gas - gasAfter - (f.childHadSteps ? f.childUsed : 0) : rec.gas;
      if (!f.childHadSteps) childGas = f.childUsed;
    }
    if (memAfter >= 0) {
      if (memAfter > rec.mem) memexp = this.memCost(memAfter) - this.memCost(rec.mem);
    } else if (op === 0xf3 || op === 0xfd) {
      memexp = cost;
    }
    if (memexp > cost) memexp = cost;
    rec.g = cost - memexp - childGas;
    rec.done = true;
    if (memexp) this.add("mem_expansion", memexp);
    if (childGas) this.add("precompile", childGas);
    this.add(rec.k, rec.g);
    this.exec += cost;
  },

  finish: function (f) {
    if (f.pend) { this.finalize(f, f.pend, -1, -1); f.pend = null; }
  },

  step: function (log, db) {
    if (!this.init) this.setup();
    var depth = log.getDepth();
    var gas = log.getGas();
    if (this.firstGas < 0) this.firstGas = gas;
    // Frames deeper than this one have ended.
    for (var d = this.frames.length - 1; d > depth; d--) {
      if (this.frames[d]) this.finish(this.frames[d]);
      this.frames[d] = undefined;
      this.frames.length = d;
    }
    var f = this.frame(log, depth);
    var mem = log.memory.length();
    if (f.pend) { this.finalize(f, f.pend, mem, gas); f.pend = null; }
    this.refund = log.getRefund();
    var op = log.op.toNumber(), pc = log.getPC();
    var rec = { op: op, k: this.baseKind[op], cost: log.getCost(), gas: gas, mem: mem, g: 0, done: false, ins: null };
    f.childUsed = 0; f.childHadSteps = false;
    var st = log.stack, sh = f.sh, npop = this.pops[op], npush = this.pushes[op], i;

    // A JUMPDEST starts a basic block: constants do not carry across it.
    if (op === 0x5b) for (i = 0; i < sh.length; i++) sh[i].c = false;

    // MLOAD/MSTORE at the free-memory pointer.
    if ((op === 0x51 || op === 0x52) && this.num(st.peek(0)) === 0x40) {
      rec.k = "fmp";
      this.fmpChain(sh[sh.length - 1]);
      if (op === 0x52) this.fmpChain(sh[sh.length - 2]);
    }

    // Revert guards.
    if (op === 0x57) {
      if (!f.info) f.info = this.codeInfo(f.addr, db);
      var dest = this.num(st.peek(0));
      if (f.info.len > 0 && ((dest >= 0 && this.revertish(f.info, dest)) || this.revertish(f.info, pc + 1))) {
        rec.k = "guard";
        this.guardChain(sh[sh.length - 1], 0);
        this.guardChain(sh[sh.length - 2], 0);
      }
    }

    // Shadow stack: which step produced each value, and whether it is a block-local constant.
    if (op >= 0x80 && op <= 0x8f) {
      var src = sh[sh.length - (op - 0x7f)];
      sh.push({ r: rec, c: src ? src.c : false });
    } else if (op >= 0x90 && op <= 0x9f) {
      var top = sh.length - 1, other = top - (op - 0x8f);
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
        sh.push({ r: rec, c: this.isPush(op) });
      }
    }
    if (op === 0x56 || op === 0x57) for (i = 0; i < sh.length; i++) sh[i].c = false;
    f.pend = rec;
  },

  enter: function (frame) {
    this.pendingTo = toHex(frame.getTo());
    this.enterExec.push(this.exec);
  },

  exit: function (res) {
    // The parent frame is the deepest frame that is not the exiting child.
    var parentDepth = this.enterExec.length;
    var execBefore = this.enterExec.pop();
    var p = this.frames[parentDepth];
    var child = this.frames[parentDepth + 1];
    if (child) this.finish(child);
    if (p) {
      // A callee with steps is charged what its steps counted, so the gas that no step carries (the
      // code deposit of a create, gas burned by a failure) stays on the call step. A callee without
      // steps is a precompile or an empty account, charged by the gas it used.
      p.childUsed = child ? this.exec - execBefore : res.getGasUsed();
      p.childHadSteps = !!child;
    }
    // Clear the entry itself: truncating `length` alone leaves it readable in the JS engine.
    this.frames[parentDepth + 1] = undefined;
    this.frames.length = parentDepth + 1;
    this.pendingTo = null;
  },

  // Anvil reports a step that reverts or fails here instead of through `step`.
  fault: function (log, db) {
    this.step(log, db);
  },

  result: function (ctx, db) {
    for (var d = this.frames.length - 1; d >= 1; d--) {
      if (this.frames[d]) this.finish(this.frames[d]);
    }
    return { kinds: this.kinds, exec: this.exec, refund: this.refund, firstGas: this.firstGas,
      gasLimit: ctx.gas };
  }
}
