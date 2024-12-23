/// [`strum`] -> [`serde`] adapter.
#[cfg(feature = "serde")]
pub(crate) struct StrumVisitor<T>(std::marker::PhantomData<T>);

#[cfg(feature = "serde")]
impl<T: std::str::FromStr + strum::VariantNames> StrumVisitor<T> {
    pub(crate) fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

#[cfg(feature = "serde")]
impl<T: std::str::FromStr + strum::VariantNames> serde::de::Visitor<'_> for StrumVisitor<T> {
    type Value = T;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<T>();
        let name = name.rsplit("::").next().unwrap_or(name);
        write!(f, "a {name} string")
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        T::from_str(v).map_err(|_| serde::de::Error::unknown_variant(v, T::VARIANTS))
    }
}
