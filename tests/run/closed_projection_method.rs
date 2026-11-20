trait Host {
    type Device: Device;
}
trait Device {
    type Stream;
}
struct Root;
struct Card;
struct AudioStream(u32);
struct Stream(AudioStream);
impl Host for Root {
    type Device = Card;
}
impl Device for Card {
    type Stream = AudioStream;
}
impl From<<<Root as Host>::Device as Device>::Stream> for Stream {
    fn from(value: <<Root as Host>::Device as Device>::Stream) -> Self {
        Stream(value)
    }
}
fn main() {
    println!("{}", Stream::from(AudioStream(42)).0.0);
}
