//! Serde serializers and deserializers.

pub(crate) mod display_fromstr {
    pub(crate) mod vec {
        use std::{fmt, marker::PhantomData, str::FromStr};

        use serde::{
            Deserializer,
            de::{self, SeqAccess, Visitor},
        };

        pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
        where
            T: FromStr,
            T::Err: fmt::Display,
            D: Deserializer<'de>,
        {
            struct VecVisitor<T> {
                marker: PhantomData<T>,
            }

            impl<'de, T> Visitor<'de> for VecVisitor<T>
            where
                T: FromStr,
                T::Err: fmt::Display,
            {
                type Value = Vec<T>;

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str("a sequence")
                }

                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: SeqAccess<'de>,
                {
                    let mut values = Vec::<T>::with_capacity(seq.size_hint().unwrap_or(0));
                    while let Some(value) = seq.next_element::<String>()? {
                        values.push(T::from_str(&value).map_err(de::Error::custom)?);
                    }
                    Ok(values)
                }
            }

            let visitor = VecVisitor { marker: PhantomData };
            deserializer.deserialize_seq(visitor)
        }
    }
}
