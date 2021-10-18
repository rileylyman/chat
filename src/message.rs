use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Message {
    pub author: String,
    pub content: String,
}

impl Message {
    pub fn write_out(&self, buf: impl std::io::Write) {
        self.serialize(&mut Serializer::new(buf)).unwrap();
    }

    pub fn read_in(buf: impl std::io::Read) -> Self {
        Message::deserialize(&mut Deserializer::new(buf)).unwrap()
    }
}
