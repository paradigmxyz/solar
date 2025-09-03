//! Serde serializers and deserializers.

pub(crate) mod display_fromstr {
    use std::{fmt::Display, str::FromStr};

    use serde::{Deserialize, Deserializer, Serializer, de};

    pub(crate) fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }

    pub(crate) mod vec {
        use std::{
            fmt::{self, Display},
            marker::PhantomData,
            str::FromStr,
        };

        use serde::{
            Deserializer, Serializer,
            de::{SeqAccess, Visitor},
            ser::SerializeSeq,
        };

        pub(crate) fn serialize<T, S>(value: &[T], serializer: S) -> Result<S::Ok, S::Error>
        where
            T: Display,
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(value.len()))?;
            for val in value {
                seq.serialize_element(&val.to_string())?;
            }
            seq.end()
        }

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
                    while let Some(value) = seq.next_element::<&str>()? {
                        values.push(T::from_str(value).map_err(serde::de::Error::custom)?);
                    }
                    Ok(values)
                }
            }

            let visitor = VecVisitor { marker: PhantomData };
            deserializer.deserialize_seq(visitor)
        }
    }
}
