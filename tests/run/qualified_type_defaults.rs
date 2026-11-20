trait Reader {
    type Offset;
}

struct ByteReader;

impl Reader for ByteReader {
    type Offset = u8;
}

struct Header<R: Reader, Offset = <R as Reader>::Offset>(R, Offset);

fn main() {
    let header: Header<ByteReader> = Header(ByteReader, 18);
    println!("{}", header.1);
}
