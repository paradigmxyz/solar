//! Pure scalar plugin builtins lower to schema-declared word operations. A bounded recipe may
//! reference arguments, constants and prior results only. The host verifies an isolated MIR
//! function even when optional IR validation is disabled, then emits the same recipe into the
//! caller. Memory, environment, control flow, calls and pointer operations are excluded.

use super::*;
use crate::mir::{EffectKind, ResultKind, Semantics};
use solar_pass_plugin::{BuiltinBody, BuiltinOperand};

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_plugin_builtin(
        &mut self,
        expr: &hir::Expr<'_>,
        namespace: Ident,
        name: Ident,
        args: &[hir::Expr<'_>],
    ) -> Option<Word> {
        let result = (|| -> Result<_, String> {
            let plugins = self.cx.gcx.plugins().map_err(|_| "failed to load plugins")?;
            let mut owner = None;
            for plugin in plugins {
                if plugin.manifest()?.name == namespace.name.as_str() {
                    owner = Some(plugin);
                    break;
                }
            }
            let body =
                owner.ok_or("unknown plugin builtin owner")?.lower_builtin(name.name.as_str())?;
            if body.instructions.len() > 4096 {
                return Err("plugin builtin exceeds instruction limit".into());
            }
            let mut function = crate::mir::Function::new(name);
            function.set_return_type(MirType::I256);
            let mut builder = FunctionBuilder::new_semantic(&mut function);
            let arguments =
                args.iter().map(|_| builder.add_param(MirType::I256)).collect::<Vec<_>>();
            let value = emit_recipe(&mut builder, &arguments, &body)?;
            builder.ret([value]);
            let mut module = crate::mir::Module::new(namespace);
            module.add_function(function);
            crate::mir::validate(self.cx.gcx.dcx(), &module);
            self.cx.gcx.dcx().has_errors().map_err(|_| "plugin builtin produced invalid MIR")?;
            let mut arguments = Vec::new();
            for arg in args {
                let value = self.operand(arg).ok_or("cannot lower plugin builtin argument")?.id();
                // argument = zext value to i256
                arguments.push(self.builder.cast(value, MirType::I256));
            }
            emit_recipe(&mut self.builder, &arguments, &body)
        })();
        match result {
            Ok(value) => Some(Word::from_clean(value)),
            Err(error) => {
                self.cx
                    .gcx
                    .dcx()
                    .err_at(
                        expr.span,
                        format!("plugin builtin `@{namespace}.{name}` failed: {error}"),
                    )
                    .emit();
                None
            }
        }
    }
}

fn emit_recipe(
    builder: &mut FunctionBuilder<'_>,
    arguments: &[ValueId],
    body: &BuiltinBody,
) -> Result<ValueId, String> {
    let mut locals = Vec::new();
    for instruction in &body.instructions {
        let operands = instruction
            .operands
            .iter()
            .map(|operand| recipe_operand(builder, arguments, &locals, operand))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = InstKind::from_operands(&instruction.operation, &operands)
            .ok_or("unknown operation or invalid operand count")?;
        // Only schema-declared pure word opcodes may implement a scalar builtin.
        if kind.effect_kind() != EffectKind::Pure
            || !matches!(kind.op_def().result, ResultKind::Integer | ResultKind::I256)
            || !matches!(kind.semantics(), Some(Semantics::Opcode(..)))
        {
            return Err("plugin builtin operation is not a pure word opcode".into());
        }
        // local = op operands
        locals.push(builder.emit_inst_unfolded(kind, Some(MirType::I256)));
    }
    recipe_operand(builder, arguments, &locals, &body.result)
}

fn recipe_operand(
    builder: &mut FunctionBuilder<'_>,
    arguments: &[ValueId],
    locals: &[ValueId],
    operand: &BuiltinOperand,
) -> Result<ValueId, String> {
    match operand {
        BuiltinOperand::Argument(index) => arguments
            .get(*index as usize)
            .copied()
            .ok_or_else(|| "invalid builtin argument index".into()),
        BuiltinOperand::Local(index) => locals
            .get(*index as usize)
            .copied()
            .ok_or_else(|| "builtin local must refer to a prior instruction".into()),
        BuiltinOperand::Constant(value) => value
            .parse::<U256>()
            .map(|word| builder.imm(word))
            .map_err(|_| "invalid builtin word constant".into()),
    }
}
