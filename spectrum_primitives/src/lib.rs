pub mod bytes;
pub mod dpf;
pub mod field;
pub mod group;
pub mod lss;
pub mod prg;
pub mod vdpf;

pub mod proto {
    tonic::include_proto!("spectrum_primitives");
}
