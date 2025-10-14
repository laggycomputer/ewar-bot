// https://gist.github.com/ImTheSquid/b5f34c39c5c4a7760b3917c394b9ec07

pub(crate) mod chrono_datetime_option_as_bson_datetime_option {
    use bson::{Bson, DateTime};
    use chrono::Utc;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::result::Result;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<chrono::DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Bson::deserialize(deserializer)? {
            Bson::Null => Ok(None),
            Bson::DateTime(dt) => Ok(Some(dt.to_chrono())),
            _ => Err(D::Error::custom("expecting DateTime or Option<DateTime>")),
        }
    }

    pub fn serialize<S: Serializer>(
        val: &Option<chrono::DateTime<Utc>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match val {
            None => None::<DateTime>.serialize(serializer),
            Some(val) => {
                let datetime = DateTime::from_chrono(val.to_owned());
                datetime.serialize(serializer)
            }
        }
    }
}
