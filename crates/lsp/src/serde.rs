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
}

pub(crate) mod display_fromstr {
    pub(crate) mod vec {
        use serde::{Deserialize, Deserializer, de};
        use std::{fmt, str::FromStr};

        pub(crate) fn deserialize<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
        where
            T: FromStr,
            T::Err: fmt::Display,
            D: Deserializer<'de>,
        {
            Vec::<String>::deserialize(deserializer)?
                .into_iter()
                .map(|value| value.parse().map_err(de::Error::custom))
                .collect()
        }
    }
}
