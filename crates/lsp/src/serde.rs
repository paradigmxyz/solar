//! Serde serializers and deserializers.

pub(crate) mod optional_display_fromstr {
    use serde::{Deserialize, Deserializer, de};
    use std::{fmt::Display, str::FromStr};

    pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| value.parse().map_err(de::Error::custom))
            .transpose()
    }

    pub(crate) mod vec {
        use serde::{Deserialize, Deserializer, de};
        use std::{fmt, str::FromStr};

        pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
        where
            T: FromStr,
            T::Err: fmt::Display,
            D: Deserializer<'de>,
        {
            Option::<Vec<String>>::deserialize(deserializer)?
                .map(|values| {
                    values
                        .into_iter()
                        .map(|value| value.parse().map_err(de::Error::custom))
                        .collect()
                })
                .transpose()
        }
    }
}
