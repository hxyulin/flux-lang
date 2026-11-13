trait Index:Copy{}impl Index for u8{}
#[derive(Clone,Copy)]struct Id<I:Index>(I);
trait Data {type Weight;type Node;}
impl<G:Data>Data for &G{type Weight=G::Weight;type Node=G::Node;}
trait NodeRef:Copy{type Weight;type Node;}
trait IntoRefs:Data{type Ref:NodeRef<Weight=Self::Weight,Node=Self::Node>;type Refs:Iterator<Item=Self::Ref>;}
struct Graph<N,I:Index>(N,I);
impl<N,I:Index>Data for Graph<N,I>{type Weight=N;type Node=Id<I>;}
impl<N,I:Index>NodeRef for (Id<I>,&N){type Weight=N;type Node=Id<I>;}
struct Iter<'a,N:'a,I:Index>{iter:std::slice::Iter<'a,N>,id:I}
impl<'a,N,I:Index>Iterator for Iter<'a,N,I>{type Item=(Id<I>,&'a N);fn next(&mut self)->Option<Self::Item>{self.iter.next().map(|x|(Id(self.id),x))}}
impl<'a,N,I:Index>IntoRefs for &'a Graph<N,I>{type Ref=(Id<I>,&'a N);type Refs=Iter<'a,N,I>;}
type DiGraph<N,I>=Graph<N,I>;
struct Acyclic<G>(G);
impl<G:Data>Data for Acyclic<G>{type Weight=G::Weight;type Node=G::Node;}
impl<'a,N,I:Index>IntoRefs for &'a Acyclic<DiGraph<N,I>>{type Ref=<&'a DiGraph<N,I> as IntoRefs>::Ref;type Refs=<&'a DiGraph<N,I> as IntoRefs>::Refs;}
trait Edges:IntoRefs{type Edges:Iterator<Item=Self::Ref>;fn edges(self)->Self::Edges;}
impl<'a,N,I:Index>Edges for &'a Graph<N,I>{type Edges=Iter<'a,N,I>;fn edges(self)->Self::Edges{Iter{iter:std::slice::from_ref(&self.0).iter(),id:self.1}}}
impl<'a,N,I:Index>Edges for &'a Acyclic<DiGraph<N,I>>{type Edges=<&'a DiGraph<N,I> as Edges>::Edges;fn edges(self)->Self::Edges{(&self.0).edges()}}
fn main(){let graph=Acyclic(Graph(7u8,1u8));let (id,value)=(&graph).edges().next().unwrap();assert_eq!(id.0,1);assert_eq!(*value,7);println!("selected header replay {}",value);}
