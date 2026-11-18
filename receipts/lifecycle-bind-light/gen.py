"""Light-build harness (not part of the branch): writes the bound Lifecycle and SourceSame modules for
Counter from T4's run-v3 receipt literals (its Lifecycle.lean.txt init/runtime NatCode values and its
Source.lean.txt, the solc-AST front end's output for the same Counter.sol) into a clone of
solidity-lean at the expected theorem's commit, as generic.py's lifecycle_build lays them out."""
import json, pathlib, re, sys
sys.path.insert(0, "/home/hand/h2/lb/tools/certify")
import generic
W = pathlib.Path(sys.argv[1]); R = pathlib.Path(sys.argv[2]); name = "counter_f1c87a33"
lt = (R / "Lifecycle.lean.txt").read_text()
def natcode(defname):
    m = re.search(rf"^def {defname} : ByteArray := NatCode\.ofNat (\d+) (\d+)$", lt, re.M)
    n, size = int(m[1]), int(m[2]); return n.to_bytes(size, "big")
init, runtime = natcode("init"), natcode("runtime")
bind = generic.lifecycle_binding(sys.argv[3], [8])
sigs = [{"sig": "bump(uint256)", "sel": 0xB20EB4C4}, {"sig": "count()", "sel": 0x06661ABD},
        {"sig": "increment()", "sel": 0xD09DE08A}, {"sig": "limit()", "sel": 0xA4D66DAF},
        {"sig": "max(uint256,uint256)", "sel": 0x6D5433E6}]
text = generic.lifecycle_lean(f"Certified.{name}.Lifecycle", f"{name}.Source", f"Certified.{name}.Src.importedSourceUnit",
                              "Counter", sigs, init, runtime, runtime, [8], [(1, 8)], bind=bind)
d = W / "Certified" / name; d.mkdir(parents=True, exist_ok=True)
src = (R / "Source.lean.txt").read_text().replace("CounterRun", name)
(d / "Source.lean").write_text(src)
(d / "Lifecycle.lean").write_text(text)
(d / "SourceSame.lean").write_text(generic.source_same_lean(name, bind))
(d / "Composed.lean").write_text(generic.composed_lean(name, bind))
generic.declare_lean_lib(W / "lakefile.lean", name,
    f'\nlean_lib «{name}» where\n  srcDir := "Certified"\n  globs := #[.submodules \u0060{name}]\n', (generic.lean_lib_body(name),))
print("init", len(init), "runtime", len(runtime))
