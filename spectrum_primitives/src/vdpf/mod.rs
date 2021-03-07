#[macro_use]
mod definition;

pub use definition::VDPF;

mod field;
mod insecure;
mod multi_key;
mod two_key;

pub use field::FieldVDPF;

/*
use crate::prg::aes::AESPRG;
use crate::prg::group::GroupPRG;
// TODO(sss) make this more abstract? Don't think that we need both MultiKeyVDPF and BasicVDPF
// should be able to just use abstract DPF notion + properties on PRG seeds (addition)
pub type BasicVdpf = FieldVDPF<BasicDPF<AESPRG>, GroupElement>;
pub type MultiKeyVdpf = FieldVDPF<MultiKeyDPF<GroupPRG<GroupElement>>, GroupElement>;
*/
