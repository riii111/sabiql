pub mod dot;
#[allow(
    unreachable_pub,
    reason = "Graphviz public trait seam is retained for API09-012"
)]
pub(crate) mod graphviz;

pub use dot::DotExporter;
