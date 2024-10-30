use protobuf::{EnumOrUnknown, Message};
include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
use timestamp::TimeStamps;

fn main() {
    println!("Hello, world!");
}
