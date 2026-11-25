"""Propose sparse stack-word invariants. Lean checks every incoming edge."""

import json
from collections import defaultdict

from . import cfg_flow, stack_layout
from .common import Unsupported


class Facts:
    def __init__(self, model, catalog, protected):
        self.model = model
        self.signatures = stack_layout.signatures(model, catalog)
        self.protected = protected
        self.predecessors = defaultdict(list)
        self.words = {}
        self.specials = {}
        for name in self.signatures:
            if name.startswith("dup "):
                self.specials[name] = f".dup {int(name[4:]) - 1}"
            elif name.startswith("swap "):
                self.specials[name] = f".swap {name[5:]}"
        for row in model["nodes"]:
            node = row["node"]
            for edge in ("next", "yes", "no"):
                if edge in node:
                    self.predecessors[node[edge]].append(row["id"])

    def demand(self, pc, depth, word):
        pending = [(pc, depth)]
        confirmed = set()
        for point, index in pending:
            if (point, index) in confirmed:
                continue
            if len(confirmed) >= 16384 or index >= 1024:
                raise Unsupported("constant stack invariant exceeds its budget")
            if point == self.model["entry"]:
                raise Unsupported("address fold depends on an incoming stack word")
            confirmed.add((point, index))
            for parent in self.predecessors[point]:
                node = self.model["nodes"][parent]["node"]
                name = node.get("operation", "")
                if node["kind"] == "jump":
                    previous = index
                elif node["kind"] == "branch":
                    previous = index + 1
                elif name.startswith("dup "):
                    previous = int(name[4:]) - 1 if index == 0 else index - 1
                elif name.startswith("swap "):
                    other = int(name[5:])
                    previous = other if index == 0 else 0 if index == other else index
                else:
                    inputs, outputs = self.signatures[name]
                    if index < outputs:
                        if parent not in self.protected and name == f"push {word}":
                            self.specials[name] = f".literal {word}#256"
                            continue
                        raise Unsupported("address fold changes an unproved stack word")
                    previous = index - outputs + inputs
                pending.append((parent, previous))
        for point, index in confirmed:
            self.words.setdefault(point, set()).add((index, word))

    def declarations(self, name, catalog):
        rows = [
            "["
            + ",".join(
                f"({depth},{word}#256)" for depth, word in sorted(self.words.get(i, ()))
            )
            + "]"
            for i in range(len(self.model["nodes"]))
        ]
        special = ",".join(
            f"({json.dumps(op)},{term})" for op, term in sorted(self.specials.items())
        )
        return (
            f"\nnamespace {name}WordFacts\n"
            + cfg_flow.table("rows", "EVM.StackWords.Facts", rows, lookup=False)
            + cfg_flow.signatures_declaration(self.signatures, catalog)
            + f"""
open EVM.StackWords
def specials (name : String) : Option Movement :=
  ([{special}] : List (String × Movement)).lookup name
def facts (i : Nat) : Facts := (rows.get? i).getD []
def check := closedAt {name}GraphNodes.view signatures specials {name}Protected facts
theorem closed : (List.range {name}GraphNodes.size).all check = true := by decide +kernel
theorem constantAt {{pc : Nat}} {{stack : EVM.Stack}}
    (reach : Reaches {name}GraphNodes.view signatures specials {name}Protected {name}Graph.entry pc stack) :
    Holds (facts pc) stack :=
  (invariant (by decide +kernel) (by decide +kernel)
    (fun i inside => List.all_eq_true.mp closed i (List.mem_range.mpr inside)) reach).2
end {name}WordFacts
"""
        )


def fold_proof(index, pc, offset, depth, base, version):
    return f"""
def addressFold{index} : AddressFold := ⟨{offset}#256,{depth},{base}#256⟩
theorem foldAt{index} (ctx : EVM.Context) (stack : EVM.Stack) (gas : Nat)
    (reach : EVM.StackWords.Reaches beforeGraphNodes.view beforeWordFacts.signatures
      beforeWordFacts.specials beforeProtected beforeGraph.entry {pc} stack)
    (room : stack.length + 2 ≤ 1024) (paid : EVM.Prefix.cost .{version} addressFold{index}.ops ≤ gas) :
    EVM.Prefix.charged ctx .{version} addressFold{index}.ops stack gas =
      some ((addressFold{index}.base + addressFold{index}.offset) :: stack,
        gas - EVM.Prefix.cost .{version} addressFold{index}.ops) :=
  addressFold{index}.funded ctx .{version} stack gas
    (beforeWordFacts.constantAt reach ({depth},{base}#256) (by decide +kernel)) room paid
"""
