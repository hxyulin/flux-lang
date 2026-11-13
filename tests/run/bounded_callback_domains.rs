trait Parser<I>{type Output;fn parse_next(&mut self,input:&mut I)->Self::Output;}
impl<I,O,F:FnMut(&mut I)->O>Parser<I>for F{type Output=O;fn parse_next(&mut self,input:&mut I)->O{self(input)}}
struct Value<F,V>{parser:F,value:V}
impl<I,F:Parser<I>,V:Clone>Parser<I>for Value<F,V>{type Output=V;fn parse_next(&mut self,input:&mut I)->V{self.parser.parse_next(input);self.value.clone()}}
struct Take;
impl<'a>Parser<&'a[u8]>for Take{type Output=&'a[u8];fn parse_next(&mut self,input:&mut &'a[u8])->&'a[u8]{*input}}
struct Alt<A,B>(A,B);
impl<I,O,A:Parser<I,Output=O>,B:Parser<I,Output=O>>Parser<I>for Alt<A,B>{type Output=O;fn parse_next(&mut self,input:&mut I)->O{self.1.parse_next(input)}}
fn newline(_: &mut &[u8]){}
fn parse(input:&mut &[u8]){let mut parser=Alt(Value{parser:newline,value:&b"\n"[..]},Take);let _=parser.parse_next(input);}
fn main(){let bytes=[1u8,2,3];let mut input=&bytes[..];parse(&mut input);println!("{}",input.len());}
