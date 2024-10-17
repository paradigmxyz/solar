use super::{Gcx, Ty};
use crate::ty::TyKind;
use std::fmt;

impl<'gcx> Gcx<'gcx> {
    /// Formats the ABI signature of a function in the form `{name}({tys},*)`.
    pub(super) fn mk_abi_signature(
        self,
        name: &str,
        tys: impl IntoIterator<Item = Ty<'gcx>>,
    ) -> String {
        let mut s = String::with_capacity(64);
        s.push_str(name);
        TyAbiPrinter::new(self, &mut s).print_tuple(tys).unwrap();
        s
    }
}

struct TyAbiPrinter<'gcx, W> {
    gcx: Gcx<'gcx>,
    buf: W,
}

impl<'gcx, W: fmt::Write> TyAbiPrinter<'gcx, W> {
    fn new(gcx: Gcx<'gcx>, buf: W) -> Self {
        Self { gcx, buf }
    }

    fn print(&mut self, ty: Ty<'gcx>) -> fmt::Result {
        debug_assert!(ty.can_be_exported(), "{ty:?} cannot be exported");
        match ty.kind {
            TyKind::Elementary(ty) => ty.write_abi_str(&mut self.buf),
            TyKind::Contract(_) => self.buf.write_str("address"),
            TyKind::FnPtr(_) => self.buf.write_str("function"),
            TyKind::Struct(id) => self.print_tuple(self.gcx.struct_field_types(id).iter().copied()),
            TyKind::Enum(_) => self.buf.write_str("uint8"),
            TyKind::Udvt(ty, _) => self.print(ty),
            TyKind::Ref(ty, _loc) => self.print(ty),
            TyKind::DynArray(ty) => {
                self.print(ty)?;
                self.buf.write_str("[]")
            }
            TyKind::Array(ty, len) => {
                self.print(ty)?;
                write!(self.buf, "[{len}]")
            }
            _ => panic!("printing invalid type: {ty:?}"),
        }
    }

    fn print_tuple(&mut self, tys: impl IntoIterator<Item = Ty<'gcx>>) -> fmt::Result {
        self.buf.write_str("(")?;
        for (i, ty) in tys.into_iter().enumerate() {
            if i > 0 {
                self.buf.write_str(",")?;
            }
            self.print(ty)?;
        }
        self.buf.write_str(")")
    }
}
